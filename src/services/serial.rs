use std::path::Path;
use std::time::Duration;

use gcinput::Input;
use serialport5::{SerialPort, SerialPortBuilder};
use tokio::sync::{oneshot, watch};
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
    let mut serial_opt: Option<SerialPort> = None;

    let mut each_second = tokio::time::interval(Duration::from_secs(1));

    // TODO(Sirius902) If there is a serial connection, asynchronously wait on the fd for data and
    // isolate a single input message.
    loop {
        tokio::select! {
            tx = &mut rx_shutdown => {
                if let Ok(tx) = tx {
                    tx.send(()).expect("sending shutdown signal");
                }
                info!("Serial service finished");
                break;
            }
            _ = each_second.tick() => {
                if serial_opt.is_none() {
                    // TODO(Sirius902) Don't hardcode serial path.
                    let path = Path::new("/dev/ttyUSB0");

                    let serial = SerialPortBuilder::new()
                        .baud_rate(115200)
                        .open(path);

                    match serial {
                        Ok(serial) => {
                            info!("Connected to serial port {}!", path.display());
                            serial_opt = Some(serial);
                        }
                        Err(err) => {
                            warn!("Failed to connect to serial port {}: {err}", path.display());
                        }
                    }
                }
            }
        }
    }
}

// const SERIAL_RATE: Duration = Duration::from_millis(30);
//
// let mut serial_port = SerialPortBuilder::new()
//     .baud_rate(115200)
//     .read_timeout(Some(Duration::from_millis(100)))
//     .open("/dev/ttyUSB0")
//     .expect("failed to open serial port");
//
// let serial_context = Arc::new(SerialContext {
//     input: input.clone(),
//     stop_flag,
// });
//
// let serial_thread = Some(thread::spawn({
//     let context = serial_context.clone();
//     move || {
//         fn read_byte(packet: &[u8], offset: usize) -> u8 {
//             let mut b = 0u8;
//             for i in 0..8 {
//                 if (packet[i + offset] & 0xF) != 0 {
//                     b |= 1 << (7 - i);
//                 }
//             }
//             b
//         }
//
//         let mut data = Vec::new();
//         let mut first_iter = true;
//
//         while !context.stop_flag.load(Ordering::Acquire) {
//             if first_iter {
//                 first_iter = false;
//             } else {
//                 // FUTURE(Sirius902) Asynchronously wait?
//                 std::thread::sleep(SERIAL_RATE);
//             }
//
//             let bytes_to_read = serial_port.bytes_to_read().expect("bytes to read");
//             if bytes_to_read == 0 {
//                 continue;
//             }
//
//             data.resize(bytes_to_read.try_into().expect("u32 fits in usize"), 0);
//             let _ = serial_port.read(&mut data).expect("read");
//
//             let Some(last_split_pos) = data.iter().rposition(|b| *b == 0xA) else {
//                 continue;
//             };
//             let Some(snd_last_split_pos) =
//                 data.iter().take(last_split_pos).rposition(|b| *b == 0xA)
//             else {
//                 continue;
//             };
//
//             let packet_start = snd_last_split_pos + 1;
//             let packet = &data[packet_start..last_split_pos];
//
//             if packet.len() == 64 {
//                 let mut input = context.input.lock().unwrap();
//                 *input = Input {
//                     button_a: packet[7] != 0,
//                     button_b: packet[6] != 0,
//                     button_x: packet[5] != 0,
//                     button_y: packet[4] != 0,
//
//                     button_left: packet[15] != 0,
//                     button_right: packet[14] != 0,
//                     button_down: packet[13] != 0,
//                     button_up: packet[12] != 0,
//
//                     button_start: packet[3] != 0,
//                     button_z: packet[11] != 0,
//                     button_r: packet[10] != 0,
//                     button_l: packet[9] != 0,
//
//                     main_stick: Stick::new(read_byte(packet, 16), read_byte(packet, 16 + 8)),
//                     c_stick: Stick::new(read_byte(packet, 16 + 16), read_byte(packet, 16 + 24)),
//                     left_trigger: read_byte(packet, 16 + 32),
//                     right_trigger: read_byte(packet, 16 + 40),
//                 };
//             } else {
//                 warn!("Unsupported serial packet length: {}", packet.len());
//             }
//         }
//     }
// }));
