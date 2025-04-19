use std::time::Duration;

use gcinput::{Input, Stick};
use tokio::sync::{oneshot, watch};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tokio_stream::StreamExt;
use tokio_util::bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder, Framed};
use tokio_util::task::TaskTracker;
use tracing::{info, warn};

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

pub fn start(task_tracker: &TaskTracker) -> Service {
    let (tx_shutdown, rx_shutdown) = oneshot::channel();
    let (tx_input, rx_input) = watch::channel(None);

    task_tracker.spawn(run(rx_shutdown, tx_input));

    Service {
        tx_shutdown,
        rx_input,
    }
}

async fn run(
    mut rx_shutdown: oneshot::Receiver<oneshot::Sender<()>>,
    tx_input: watch::Sender<Option<Input>>,
) {
    let mut serial_opt: Option<Framed<SerialStream, LineCodec>> = None;
    let mut each_second = tokio::time::interval(Duration::from_secs(1));

    loop {
        let packet_fut = async {
            if let Some(serial) = &mut serial_opt {
                serial.next().await
            } else {
                std::future::pending().await
            }
        };

        tokio::select! {
            tx = &mut rx_shutdown => {
                if let Ok(tx) = tx {
                    tx.send(()).expect("sending shutdown signal");
                }
                info!("Serial service finished");
                break;
            }
            packet = packet_fut => {
                if let Some(packet) = packet {
                    match packet {
                        Ok(packet) => {
                            if packet.len() == 64 {
                                let new_input = Input {
                                    button_a: packet[7] != 0,
                                    button_b: packet[6] != 0,
                                    button_x: packet[5] != 0,
                                    button_y: packet[4] != 0,

                                    button_left: packet[15] != 0,
                                    button_right: packet[14] != 0,
                                    button_down: packet[13] != 0,
                                    button_up: packet[12] != 0,

                                    button_start: packet[3] != 0,
                                    button_z: packet[11] != 0,
                                    button_r: packet[10] != 0,
                                    button_l: packet[9] != 0,

                                    main_stick: Stick::new(read_byte(&packet, 16), read_byte(&packet, 16 + 8)),
                                    c_stick: Stick::new(read_byte(&packet, 16 + 16), read_byte(&packet, 16 + 24)),
                                    left_trigger: read_byte(&packet, 16 + 32),
                                    right_trigger: read_byte(&packet, 16 + 40),
                                };

                                tx_input.send_if_modified(|input| {
                                    if Some(new_input) != *input {
                                        *input = Some(new_input);
                                        true
                                    } else {
                                        false
                                    }
                                });
                            } else {
                                warn!("Unsupported serial packet length: {}", packet.len());
                            }
                        }
                        Err(err) => {
                            warn!("Error receiving packet: {err}");
                        }
                    }
                } else {
                    info!("Serial disconnected");
                    serial_opt = None;
                }
            }
            _ = each_second.tick() => {
                if serial_opt.is_none() {
                    // TODO(Sirius902) Don't hardcode serial path.
                    let path = "/dev/ttyUSB0";
                    let serial = tokio_serial::new(path, 115200).open_native_async();

                    match serial {
                        Ok(serial) => {
                            info!("Connected to serial port {path}!");
                            serial_opt = Some(LineCodec.framed(serial));
                        }
                        Err(err) => {
                            warn!("Failed to connect to serial port {path}: {err}");
                        }
                    }
                }
            }
        }
    }
}

// https://github.com/jaburns/NintendoSpy/blob/eaa649e9ba9029fa9451585d53dd73c0170bf816/Readers/SignalTool.cs#L14
fn read_byte(packet: &[u8], offset: usize) -> u8 {
    let mut b = 0u8;
    for i in 0..8 {
        if (packet[i + offset] & 0xF) != 0 {
            b |= 1 << (7 - i);
        }
    }
    b
}

struct LineCodec;

impl Decoder for LineCodec {
    type Item = Vec<u8>;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(start) = src.iter().position(|b| *b == b'\n') {
            if let Some(end) = src.iter().skip(start + 1).position(|b| *b == b'\n') {
                let end = start + 1 + end;

                let _ = src.split_to(start + 1);
                let packet = src.split_to(end - start - 1);
                let _ = src.split_to(1);

                return Ok(Some(packet.to_vec()));
            }
        }

        Ok(None)
    }
}

impl Encoder<Vec<u8>> for LineCodec {
    type Error = std::io::Error;

    fn encode(&mut self, _item: Vec<u8>, _dst: &mut BytesMut) -> Result<(), Self::Error> {
        Ok(())
    }
}
