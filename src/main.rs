#![deny(clippy::all)]
use std::{
    env, fs,
    io::Read,
    mem,
    net::UdpSocket,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use clap::Parser;
use enclose::enclose;
use gcinput::Input;
use gcviewer::state::State;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{Icon, WindowBuilder},
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
        concat!("g", env!("VERGEN_GIT_SHA_SHORT"))
    };

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(format!("gcviewer | {}", version_string))
        .with_inner_size(winit::dpi::PhysicalSize {
            width: 512,
            height: 256,
        })
        .with_window_icon(Some(icon))
        .build(&event_loop)
        .unwrap();

    let socket = UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect(("127.0.0.1", args.port)).map(|()| s))
        .and_then(|s| {
            s.set_read_timeout(Some(Duration::from_millis(100)))
                .map(|()| s)
        })
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

    let mut socket_thread = Some(thread::spawn(enclose!((context) move || {
        let input_size = bincode::serialized_size(&Input::default()).unwrap();
        let mut data = vec![0u8; input_size as usize];

        while !context.stop_flag.load(Ordering::Acquire) {
            let _ = context.socket.send(&[]);

            match context.socket.recv(&mut data) {
                Ok(received) => {
                    if received == data.len() {
                        let new_input = bincode::deserialize(&data).unwrap();
                        let mut input = context.input.lock().unwrap();
                        *input = new_input;
                    } else {
                        log::error!("Socket received incomplete data of size {}", received);
                        break;
                    }
                }
                Err(_) => {}
            }
        }
    })));

    let mut state = State::new(&window, custom_shader).await;

    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == window.id() => match event {
            WindowEvent::CloseRequested => {
                context.stop_flag.store(true, Ordering::Release);
                if let Some(t) = socket_thread.take() {
                    mem::drop(t.join());
                }

                *control_flow = ControlFlow::Exit;
            }
            WindowEvent::Resized(physical_size) => {
                state.resize(*physical_size);
            }
            WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                state.resize(**new_inner_size);
            }
            _ => {}
        },
        Event::RedrawRequested(window_id) if window_id == window.id() => {
            {
                let input = context.input.lock().unwrap();
                state.update(&input);
            }

            match state.render() {
                Ok(()) | Err(wgpu::SurfaceError::Outdated) => {}
                Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                Err(e) => log::error!("{:?}", e),
            }
        }
        Event::MainEventsCleared => {
            window.request_redraw();
        }
        _ => {}
    });
}
