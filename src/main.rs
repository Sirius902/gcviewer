use std::{
    env, fs,
    io::Read,
    mem,
    net::UdpSocket,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use clap::Parser;
use enclose::enclose;
use gcinput::Input;
use gcviewer::state::State;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::*,
    event_loop::EventLoop,
    window::{Icon, Window, WindowAttributes},
};

const ICON_FILE: &[u8] = include_bytes!("../resource/icon.png");

fn main() {
    let exe_path = env::current_exe().expect("Failed to get current exe path");
    env::set_current_dir(
        exe_path
            .parent()
            .expect("Failed to get current exe parent path"),
    )
    .expect("Failed to set current working directory");

    env_logger::init();

    let custom_shader = fs::File::open("shader.wgsl")
        .and_then(|mut f| {
            let mut s = String::new();
            f.read_to_string(&mut s).map(|_| s)
        })
        .ok();

    let args = Args::parse();
    pollster::block_on(run(&args, custom_shader));
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
    stop_flag: AtomicBool,
}

struct App<'a> {
    version_string: String,
    icon: Option<Icon>,
    custom_shader: Option<String>,
    context: Arc<SocketContext>,
    socket_thread: Option<JoinHandle<()>>,
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
                        .with_inner_size(winit::dpi::PhysicalSize {
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
                    Err(e) => log::error!("{:?}", e),
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

    let version_string = if !env!("VERSION").is_empty() {
        env!("VERSION")
    } else {
        env!("VERGEN_GIT_DESCRIBE")
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

    let context = Arc::new(SocketContext {
        socket,
        input: Default::default(),
        stop_flag: AtomicBool::new(false),
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
                    log::error!("Socket received incomplete data of size {}", received);
                    break;
                }
            }
        }
    })));

    let event_loop = EventLoop::new().unwrap();
    let mut app = App {
        version_string: version_string.to_string(),
        icon: Some(icon),
        custom_shader,
        context,
        socket_thread,
        window: Default::default(),
        state: Default::default(),
    };
    let _ = event_loop.run_app(&mut app);
}
