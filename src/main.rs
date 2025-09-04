use std::sync::Arc;

use clap::Parser;
use gcinput::Input;
use gcviewer::services;
use gcviewer::state::State;
use tokio::sync::{oneshot, watch};
use tokio_util::task::TaskTracker;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::*;
use winit::event_loop::EventLoop;
use winit::window::{Icon, Window, WindowAttributes};

const ICON_FILE: &[u8] = include_bytes!("../resource/icon.png");

fn main() {
    let _guard = setup_logging();

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));

    let args = Args::parse();

    let (tx_app, rx_app) = oneshot::channel();
    let (tx_close, rx_close) = oneshot::channel();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");

    rt.spawn(async move {
        run(
            &args,
            tx_app,
            rx_close,
            load_custom_shader(exe_dir.as_ref()).await,
        )
        .await;
    });

    let mut app = App {
        tx_close: Some(tx_close),
        ..rx_app.blocking_recv().expect("recv app")
    };
    let event_loop = EventLoop::new().expect("create event loop");
    let _ = event_loop.run_app(&mut app);
}

async fn load_custom_shader(exe_dir: Option<impl AsRef<std::path::Path>>) -> Option<String> {
    if let Some(exe_dir) = exe_dir
        && let Ok(shader) = tokio::fs::read_to_string(exe_dir.as_ref().join("shader.wgsl")).await {
            return Some(shader);
        }

    let path = directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().join("gcviewer").join("shader.wgsl"))?;

    tokio::fs::read_to_string(path).await.ok()
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

struct App<'a> {
    rt: tokio::runtime::Handle,
    tx_close: Option<oneshot::Sender<oneshot::Sender<()>>>,
    rx_socket_input: watch::Receiver<Option<Input>>,
    rx_serial_input: watch::Receiver<Option<Input>>,
    version_string: String,
    icon: Option<Icon>,
    custom_shader: Option<String>,
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
                        .with_window_icon(Some(self.icon.take().expect("icon exists"))),
                )
                .expect("create window"),
        );

        self.window = Some(window.clone());
        self.state = Some(
            self.rt
                .block_on(State::new(window, self.custom_shader.take())),
        );
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

        // TODO(Sirius902) We need a way for the tokio runtime to inject an exit event. Event loop
        // proxy?
        match event {
            WindowEvent::CloseRequested => {
                let (tx, rx) = oneshot::channel();
                self.tx_close
                    .take()
                    .expect("close channel exists")
                    .send(tx)
                    .expect("send close");

                rx.blocking_recv().expect("wait for closed");

                // FUTURE(Sirius902) Explicitly drop state before exiting event loop otherwise we
                // crash in some wayland code. Fix the surface lifetimes in [`State`] so that this won't happen?
                if let Some(state) = self.state.take() {
                    drop(state);
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
                // FUTURE(Sirius902) Give the user the option to select an input source.
                let input = (*self.rx_socket_input.borrow_and_update())
                    .or_else(|| *self.rx_serial_input.borrow_and_update());

                // FUTURE(Sirius902) Pass along the option and indicate there is no connection
                // visually.
                state.update(&input.unwrap_or_default());

                match state.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(err) => warn!("{err:?}"),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = self.window.as_ref().expect("window exists");

        let PhysicalSize { width, height } = window.inner_size();
        if width != 0 && height != 0 {
            window.request_redraw();
        }
    }
}

async fn run(
    args: &Args,
    tx_app: oneshot::Sender<App<'_>>,
    rx_close: oneshot::Receiver<oneshot::Sender<()>>,
    custom_shader: Option<String>,
) {
    let task_tracker = TaskTracker::new();

    let socket_service = services::socket::start(&task_tracker, args.port);
    let serial_service = services::serial::start(&task_tracker);

    task_tracker.close();

    let icon = {
        let icon = image::load_from_memory(ICON_FILE).expect("load icon");
        let rgba = icon.into_rgba8();
        let (width, height) = rgba.dimensions();
        Icon::from_rgba(rgba.to_vec(), width, height).expect("icon from rgba")
    };

    let app = App {
        rt: tokio::runtime::Handle::current(),
        tx_close: None,
        rx_socket_input: socket_service.watch_input(),
        rx_serial_input: serial_service.watch_input(),
        version_string: env!("GCVIEWER_VERSION").to_string(),
        icon: Some(icon),
        custom_shader,
        window: Default::default(),
        state: Default::default(),
    };

    let app_res = tx_app.send(app);

    if app_res.is_ok() {
        tokio::select! {
            // FUTURE(Sirius902) Should we handle any other signals here?
            res = tokio::signal::ctrl_c() => {
                if let Err(err) = res {
                    warn!("Failed to wait for ctrl+c signal: {err}");
                }
            }
            tx = rx_close => {
                if let Ok(tx) = tx {
                    tx.send(()).expect("sending closed signal");
                }
                info!("Handled close signal!");
            }
            _ = task_tracker.wait() => {},
        }
    } else {
        error!("Failed to send app");
    }

    info!("Stopping serial service...");
    serial_service.stop().await;
    info!("Serial service stopped!");

    info!("Stopping socket service...");
    socket_service.stop().await;
    info!("Socket service stopped!");
}

fn setup_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let builder = tracing_subscriber::registry();

    #[cfg(feature = "tokio-console")]
    let builder = builder.with(console_subscriber::spawn().with_filter({
        use tracing::level_filters::LevelFilter;

        EnvFilter::builder()
            .with_default_directive(LevelFilter::DEBUG.into())
            .parse("tokio=trace,runtime=trace")
            .expect("tokio-console env filter string parses")
    }));

    let env_filter = || {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::builder()
                .parse(["gcviewer=trace"].join(","))
                .expect("env filter string parses")
        })
    };

    let builder = builder.with(tracing_subscriber::fmt::layer().with_filter(env_filter()));

    let file_layer = directories::BaseDirs::new()
        .map(|dirs| dirs.cache_dir().join("gcviewer").join("logs"))
        .map(|logs_dir| {
            let file_appender = tracing_appender::rolling::daily(logs_dir, "gcviewer.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            (
                tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking)
                    .with_filter(env_filter()),
                guard,
            )
        });

    if let Some((file_layer, guard)) = file_layer {
        builder.with(file_layer).init();
        Some(guard)
    } else {
        builder.init();
        None
    }
}
