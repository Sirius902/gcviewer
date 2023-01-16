#![deny(clippy::all)]
use std::{
    env, io, mem,
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
    window::WindowBuilder,
};

fn main() {
    std::panic::set_hook(Box::new(panic_log::hook));

    let exe_path = env::current_exe().expect("Failed to get current exe path");
    env::set_current_dir(
        exe_path
            .parent()
            .expect("Failed to get current exe parent path"),
    )
    .expect("Failed to set current working directory");

    env_logger::init();

    let args = Args::parse();
    pollster::block_on(run(&args));
}

#[derive(Parser)]
struct Args {
    #[arg(
        short,
        long,
        default_value_t = 4096,
        help = "Uses the specified port for the UDP server."
    )]
    port: u16,
}

struct SocketContext {
    socket: UdpSocket,
    input: Arc<Mutex<Input>>,
    stop_flag: AtomicBool,
}

async fn run(args: &Args) {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(format!("gcviewer | {}", env!("VERSION")))
        .with_inner_size(winit::dpi::PhysicalSize {
            width: 512,
            height: 256,
        })
        .build(&event_loop)
        .unwrap();

    let socket = UdpSocket::bind(("127.0.0.1", args.port)).unwrap_or_else(|e| {
        panic!(
            "Failed to create to input server on localhost:{}: {}",
            args.port, e
        );
    });
    socket
        .set_nonblocking(true)
        .expect("Failed to set socket to nonblocking");

    let context = Arc::new(SocketContext {
        socket,
        input: Default::default(),
        stop_flag: AtomicBool::new(false),
    });

    let mut socket_thread = Some(thread::spawn(enclose!((context) move || {
        let input_size = bincode::serialized_size(&Input::default()).unwrap();
        let mut data = vec![0u8; input_size as usize];

        while !context.stop_flag.load(Ordering::Acquire) {
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
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(8));
                    continue;
                }
                Err(e) => log::error!("{}", e),
            }
        }
    })));

    let mut state = State::new(&window).await;

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
