use std::io::Read;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::{env, fs, mem};

use clap::Parser;
use enclose::enclose;
use gcinput::{Input, Stick};
use gcviewer::state::State;
use serialport5::SerialPortBuilder;
use tracing::warn;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::*;
use winit::event_loop::EventLoop;
use winit::window::{Icon, Window, WindowAttributes};

const ICON_FILE: &[u8] = include_bytes!("../resource/icon.png");

fn main() {
    let exe_path = env::current_exe().expect("Failed to get current exe path");
    env::set_current_dir(
        exe_path
            .parent()
            .expect("Failed to get current exe parent path"),
    )
    .expect("Failed to set current working directory");

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::builder()
                    .parse(["gcviewer=debug"].join(","))
                    .expect("env filter string parses")
            }),
        ))
        .init();

    let args = Args::parse();
    pollster::block_on(run(&args, load_custom_shader()));
}

fn load_custom_shader() -> Option<String> {
    fs::File::open("shader.wgsl")
        .ok()
        .or_else(|| {
            directories::BaseDirs::new()
                .map(|dirs| dirs.config_dir().join("gcviewer").join("shader.wgsl"))
                .and_then(|path| fs::File::open(path).ok())
        })
        .and_then(|mut f| {
            let mut s = String::new();
            f.read_to_string(&mut s).map(|_| s).ok()
        })
}

#[derive(Parser)]
struct Args {
    #[arg(
        short,
        long,
        default_value_t = 4096,
        help = "Connects to UDP server on the specified port."
    )]
    port: u16,
}

struct SocketContext {
    socket: UdpSocket,
    input: Arc<Mutex<Input>>,
    stop_flag: Arc<AtomicBool>,
}

struct SerialContext {
    input: Arc<Mutex<Input>>,
    stop_flag: Arc<AtomicBool>,
}

struct App<'a> {
    version_string: String,
    icon: Option<Icon>,
    custom_shader: Option<String>,
    context: Arc<SocketContext>,
    socket_thread: Option<JoinHandle<()>>,
    serial_thread: Option<JoinHandle<()>>,
    window: Option<Arc<Window>>,
    state: Option<State<'a>>,
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title(format!("gcviewer | {}", self.version_string))
                        .with_inner_size(winit::dpi::LogicalSize {
                            width: 512,
                            height: 256,
                        })
                        .with_window_icon(Some(self.icon.take().unwrap())),
                )
                .unwrap(),
        );

        self.window = Some(window.clone());
        self.state = Some(pollster::block_on(State::new(
            window.clone(),
            self.custom_shader.take(),
        )));
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if window_id != window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.context.stop_flag.store(true, Ordering::Release);
                if let Some(t) = self.socket_thread.take() {
                    mem::drop(t.join());
                }

                if let Some(t) = self.serial_thread.take() {
                    mem::drop(t.join());
                }

                // FUTURE(Sirius902) Explicitly drop state before exiting event loop otherwise we
                // crash in some wayland code. Fix the surface lifetimes in [`State`] so that this won't happen?
                if let Some(state) = self.state.take() {
                    mem::drop(state);
                }

                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                state.resize(physical_size);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                state.resize(window.inner_size());
            }
            WindowEvent::RedrawRequested => {
                {
                    let input = self.context.input.lock().unwrap();
                    state.update(&input);
                }

                match state.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => tracing::error!("{:?}", e),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = self.window.as_ref().unwrap();

        let PhysicalSize { width, height } = window.inner_size();
        if width != 0 && height != 0 {
            window.request_redraw();
        } else {
            thread::sleep(Duration::from_millis(16));
        }
    }
}

async fn run(args: &Args, custom_shader: Option<String>) {
    let icon = {
        let icon = image::load_from_memory(ICON_FILE).unwrap();
        let rgba = icon.into_rgba8();
        let (width, height) = rgba.dimensions();
        Icon::from_rgba(rgba.to_vec(), width, height).unwrap()
    };

    const SOCK_TIMEOUT: Duration = Duration::from_millis(100);

    let socket = UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect(("127.0.0.1", args.port)).map(|()| s))
        .and_then(|s| s.set_read_timeout(Some(SOCK_TIMEOUT)).map(|()| s))
        .and_then(|s| s.set_write_timeout(Some(SOCK_TIMEOUT)).map(|()| s))
        .unwrap_or_else(|e| {
            panic!(
                "Failed to connect to input server on localhost:{}: {}",
                args.port, e
            );
        });

    let input: Arc<Mutex<Input>> = Default::default();
    let stop_flag = Arc::new(AtomicBool::new(false));

    let context = Arc::new(SocketContext {
        socket,
        input: input.clone(),
        stop_flag: stop_flag.clone(),
    });

    let socket_thread = Some(thread::spawn(enclose!((context) move || {
        let input_size = bincode::serialized_size(&Input::default()).unwrap();
        let mut data = vec![0u8; input_size as usize];

        while !context.stop_flag.load(Ordering::Acquire) {
            let _ = context.socket.send(&[]);

            if let Ok(received) = context.socket.recv(&mut data) {
                if received == data.len() {
                    let new_input = bincode::deserialize(&data).unwrap();
                    let mut input = context.input.lock().unwrap();
                    *input = new_input;
                } else {
                    tracing::error!("Socket received incomplete data of size {}", received);
                    break;
                }
            }
        }
    })));

    const SERIAL_RATE: Duration = Duration::from_millis(30);

    let mut serial_port = SerialPortBuilder::new()
        .baud_rate(115200)
        .read_timeout(Some(Duration::from_millis(100)))
        .open("/dev/ttyUSB0")
        .expect("failed to open serial port");

    let serial_context = Arc::new(SerialContext {
        input: input.clone(),
        stop_flag,
    });

    // TODO(Sirius902) Attribute logic to NintendoSpy.
    let serial_thread = Some(thread::spawn({
        let context = serial_context.clone();
        move || {
            fn read_byte(packet: &[u8], offset: usize) -> u8 {
                let mut b = 0u8;
                for i in 0..8 {
                    if (packet[i + offset] & 0xF) != 0 {
                        b |= 1 << (7 - i);
                    }
                }
                b
            }

            let mut data = Vec::new();
            let mut first_iter = true;

            while !context.stop_flag.load(Ordering::Acquire) {
                if first_iter {
                    first_iter = false;
                } else {
                    // FUTURE(Sirius902) Asynchronously wait?
                    std::thread::sleep(SERIAL_RATE);
                }

                let bytes_to_read = serial_port.bytes_to_read().expect("bytes to read");
                if bytes_to_read == 0 {
                    continue;
                }

                data.resize(bytes_to_read.try_into().expect("u32 fits in usize"), 0);
                let _ = serial_port.read(&mut data).expect("read");

                let Some(last_split_pos) = data.iter().rposition(|b| *b == 0xA) else {
                    continue;
                };
                let Some(snd_last_split_pos) =
                    data.iter().take(last_split_pos).rposition(|b| *b == 0xA)
                else {
                    continue;
                };

                let packet_start = snd_last_split_pos + 1;
                let packet = &data[packet_start..last_split_pos];

                if packet.len() == 64 {
                    let mut input = context.input.lock().unwrap();
                    *input = Input {
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

                        main_stick: Stick::new(read_byte(packet, 16), read_byte(packet, 16 + 8)),
                        c_stick: Stick::new(read_byte(packet, 16 + 16), read_byte(packet, 16 + 24)),
                        left_trigger: read_byte(packet, 16 + 32),
                        right_trigger: read_byte(packet, 16 + 40),
                    };
                } else {
                    warn!("Unsupported serial packet length: {}", packet.len());
                }
            }
        }
    }));

    let event_loop = EventLoop::new().unwrap();
    let mut app = App {
        version_string: env!("GCVIEWER_VERSION").to_string(),
        icon: Some(icon),
        custom_shader,
        context,
        socket_thread,
        serial_thread,
        window: Default::default(),
        state: Default::default(),
    };
    let _ = event_loop.run_app(&mut app);
}
