use std::sync::Arc;
use std::time::Duration;

use gcinput::Input;
use tokio::net::UdpSocket;
use tokio::sync::{oneshot, watch};
use tokio_util::task::TaskTracker;
use tracing::{debug, info, warn};

pub struct Service {
    tx_shutdown: oneshot::Sender<oneshot::Sender<()>>,
    rx_input: watch::Receiver<Option<Input>>,
}

impl Service {
    pub async fn stop(self) {
        let (tx, rx) = oneshot::channel();
        self.tx_shutdown.send(tx).expect("sending shutdown signal");
        rx.await.expect("waiting for shutdown");
    }

    pub fn watch_input(&self) -> watch::Receiver<Option<Input>> {
        self.rx_input.clone()
    }
}

pub fn start(task_tracker: &TaskTracker, port: u16) -> Service {
    let (tx_shutdown, rx_shutdown) = oneshot::channel();
    let (tx_input, rx_input) = watch::channel(None);

    task_tracker.spawn(run(rx_shutdown, tx_input, port));

    Service {
        tx_shutdown,
        rx_input,
    }
}

async fn run(
    mut rx_shutdown: oneshot::Receiver<oneshot::Sender<()>>,
    tx_input: watch::Sender<Option<Input>>,
    port: u16,
) {
    let mut socket_opt: Option<Arc<UdpSocket>> = None;
    let mut each_second = tokio::time::interval(Duration::from_secs(1));

    // FUTURE(Sirius902) This seems brittle, send json or use protobufs?
    let input_size =
        usize::try_from(bincode::serialized_size(&Some(Input::default())).expect("Input size"))
            .expect("Input size fits in usize");
    let mut data = vec![0u8; input_size];

    loop {
        let data_fut = {
            let socket_opt = socket_opt.clone();
            let data = &mut data;
            async move {
                if let Some(socket) = &socket_opt {
                    socket.recv(data).await
                } else {
                    std::future::pending().await
                }
            }
        };

        tokio::select! {
            tx = &mut rx_shutdown => {
                if let Ok(tx) = tx {
                    tx.send(()).expect("sending shutdown signal");
                }
                info!("Socket service finished");
                break;
            }
            res = data_fut => {
                match res {
                    Ok(len) => {
                        if len == data.len() {
                            let new_input: Option<Input> = match bincode::deserialize(&data) {
                                Ok(input) => input,
                                Err(err) => {
                                    warn!("Failed to deserialize input: {err}");
                                    continue;
                                }
                            };

                            tx_input.send_if_modified(|input| {
                                if new_input != *input {
                                    *input = new_input;
                                    true
                                } else {
                                    false
                                }
                            });
                        } else {
                            warn!("Received incomplete input message of len: {len}");
                        }
                    }
                    Err(err) => {
                        warn!("Failed to read data: {err}");
                    }
                }
            }
            _ = each_second.tick() => {
                if let Some(socket) = &socket_opt {
                    // Send heartbeat.
                    if let Err(err) = socket.send(&[]).await {
                        warn!("Failed to send heartbeat to localhost:{port}: {err}");
                    }
                } else {
                    // Connect the socket.
                    let socket = match UdpSocket::bind("0.0.0.0:0").await {
                        Ok(socket) => socket,
                        Err(err) => {
                            debug!("Failed to bind server, trying again in 1s: {err}");
                            continue;
                        }
                    };

                    if let Err(err) = socket.connect(("127.0.0.1", port)).await {
                        debug!("Failed to connect to localhost:{port}, trying again in 1s: {err}");
                        continue;
                    }

                    info!("Connected to localhost:{port}!");
                    socket_opt = Some(Arc::new(socket));
                }
            }
        }
    }
}
