use crate::graphics_core::GraphicsCore;
use crate::timer::Timer;
use crate::{InitFlags, InitParams};

use env_logger::{self, Env};

use log::info;

use winit::{
    dpi::LogicalSize, Event, EventsLoop, KeyboardInput, VirtualKeyCode, WindowBuilder, WindowEvent,
};

#[cfg(target_os = "windows")]
use winit::os::windows::WindowExt;

pub trait GameApp {
    fn startup(&self);
    fn cleanup(&self);
    fn is_done(&self) -> bool;
    fn update(&self, timer: &Timer);
    fn render(&self);
}

pub struct GameCore {}

impl GameCore {
    pub fn run<A: GameApp>(app: A) {
        let env = Env::default()
            .filter_or("MY_LOG_LEVEL", "trace")
            .write_style_or("MY_LOG_STYLE", "auto");
        env_logger::init_from_env(env);

        // TODO: This should come as parameters to the function
        const DISPLAY_WIDTH: u32 = 1280;
        const DISPLAY_HEIGHT: u32 = 720;

        let mut event_loop = EventsLoop::new();
        let window = WindowBuilder::new()
            .with_min_dimensions(LogicalSize::new(1.0, 1.0))
            .with_dimensions(LogicalSize::new(
                f64::from(DISPLAY_WIDTH),
                f64::from(DISPLAY_HEIGHT),
            ))
            .with_title("Adamant Window")
            .build(&event_loop)
            .unwrap();

        // TODO: This should be passed as parameters to the run() function, window handle aside
        let mut params =
            InitParams::new(window.get_hwnd() as *mut _, DISPLAY_WIDTH, DISPLAY_HEIGHT);
        params.flags = InitFlags::ALLOW_TEARING | InitFlags::ENABLE_HDR;

        let mut graphics = GraphicsCore::new(params);
        let mut timer = Timer::new();

        app.startup();

        let mut is_running = true;
        while is_running {
            event_loop.poll_events(|event| match event {
                Event::WindowEvent {
                    event:
                        WindowEvent::KeyboardInput {
                            input:
                                KeyboardInput {
                                    virtual_keycode: Some(VirtualKeyCode::Escape),
                                    ..
                                },
                            ..
                        },
                    ..
                } => {
                    info!("Escape key pressed, exiting.");
                    is_running = false;
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    window_id,
                } if window_id == window.id() => {
                    info!("Window was closed, exiting.");
                    is_running = false;
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(LogicalSize { width, height }),
                    ..
                } => {
                    info!("Window size has changed.");
                    Self::on_window_size_changed(&mut graphics, width as _, height as _);
                }
                _ => (),
            });

            is_running &= Self::update(&mut timer, &app);
            if is_running {
                Self::render(&mut graphics, &app);
            }
        }

        Self::cleanup(&mut graphics, &app);
    }

    fn update(timer: &mut Timer, app: &impl GameApp) -> bool {
        timer.update(app);

        !app.is_done()
    }

    fn render(graphics: &mut GraphicsCore, app: &impl GameApp) {
        graphics.prepare();

        // TODO: Clearing will be part of app::render() as well
        graphics.clear();

        app.render();

        graphics.present();
    }

    fn on_window_size_changed(graphics: &mut GraphicsCore, width: i32, height: i32) {
        graphics.on_window_size_changed(width, height);
    }

    fn cleanup(graphics: &mut GraphicsCore, app: &impl GameApp) {
        graphics.wait_for_gpu();
        app.cleanup();
        graphics.destroy();
    }
}
