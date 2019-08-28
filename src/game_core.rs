use crate::game_timer::GameTimer;
use crate::graphics_core::GraphicsCore;
use crate::InitParams;

use env_logger::{self, Env};

use log::info;

use winit::{
    dpi::LogicalSize, Event, EventsLoop, KeyboardInput, VirtualKeyCode, Window, WindowBuilder,
    WindowEvent,
};

pub trait GameApp {
    fn startup(&mut self);
    fn cleanup(&mut self);
    fn is_done(&self) -> bool {
        false
    }
    fn update(&mut self, timer: &GameTimer);
    fn render(&self, timer: &GameTimer);
}

pub struct GameSystems {
    timer: GameTimer,
    graphics: GraphicsCore,
}

impl GameSystems {
    pub fn new(window: &Window, params: InitParams) -> Self {
        let timer = GameTimer::new();
        let graphics = GraphicsCore::new(window, params);
        GameSystems { timer, graphics }
    }
}

pub enum GameCore {}

impl GameCore {
    pub fn run<A: GameApp>(app: &mut A, params: InitParams) {
        let env = Env::default()
            .filter_or("MY_LOG_LEVEL", "trace")
            .write_style_or("MY_LOG_STYLE", "auto");
        env_logger::init_from_env(env);

        let mut event_loop = EventsLoop::new();
        let window = WindowBuilder::new()
            .with_min_dimensions(LogicalSize::new(1.0, 1.0))
            .with_dimensions(LogicalSize::new(
                f64::from(params.window_width),
                f64::from(params.window_height),
            ))
            .with_title("Adamant Window")
            .build(&event_loop)
            .unwrap();

        let mut systems = GameSystems::new(&window, params);
        let mut graphics = &mut systems.graphics;
        let mut timer = &mut systems.timer;

        app.startup();

        timer.reset();

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

            is_running &= Self::update(&mut timer, app);
            if is_running {
                Self::render(&mut graphics, &timer, app);
            }
        }

        Self::cleanup(&mut graphics, app);
    }

    fn update(timer: &mut GameTimer, app: &mut impl GameApp) -> bool {
        timer.tick();
        app.update(timer);
        !app.is_done()
    }

    fn render(graphics: &mut GraphicsCore, timer: &GameTimer, app: &impl GameApp) {
        graphics.prepare();

        // TODO: Clearing will be part of app::render() as well
        graphics.clear();

        app.render(&timer);

        graphics.present();
    }

    fn on_window_size_changed(graphics: &mut GraphicsCore, width: i32, height: i32) {
        graphics.on_window_size_changed(width, height);
    }

    fn cleanup(graphics: &mut GraphicsCore, app: &mut impl GameApp) {
        graphics.wait_for_gpu();
        app.cleanup();
        graphics.destroy();
    }
}
