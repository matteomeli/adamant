use crate::game_timer::GameTimer;
use crate::graphics::renderer::Renderer;
use crate::InitParams;

//use env_logger::{self, Env};
//use log::info;

use winit::{
    dpi::LogicalSize, Event, EventsLoop, KeyboardInput, VirtualKeyCode, Window, WindowBuilder,
    WindowEvent,
};

pub trait GameApp {
    fn get_params(&self) -> InitParams;
    fn activate(&mut self);
    fn deactivate(&mut self);
    fn update(&mut self, timer: &GameTimer);
    fn render(&self, timer: &GameTimer);
}

pub struct GameSystems {
    timer: GameTimer,
    renderer: Renderer,
}

impl GameSystems {
    pub fn new(window: &Window, params: &InitParams) -> Self {
        let timer = GameTimer::new();
        let renderer = Renderer::new(window, params);
        GameSystems { timer, renderer }
    }
}

pub struct GameCore<A: GameApp> {
    app: A,
}

impl<A: GameApp> GameCore<A> {
    pub fn new(app: A) -> Self {
        GameCore { app }
    }

    pub fn run(mut self) {
        //let env = Env::default()
        //    .filter_or("MY_LOG_LEVEL", "trace")
        //    .write_style_or("MY_LOG_STYLE", "auto");
        //env_logger::init_from_env(env);

        let params = &self.app.get_params();

        let mut event_loop = EventsLoop::new();
        let window = WindowBuilder::new()
            .with_min_dimensions(LogicalSize::new(1.0, 1.0))
            .with_dimensions(LogicalSize::new(
                f64::from(params.window_width),
                f64::from(params.window_height),
            ))
            .with_title(&params.window_title)
            .build(&event_loop)
            .unwrap();

        let mut systems = GameSystems::new(&window, params);
        let renderer = &mut systems.renderer;
        let timer = &mut systems.timer;

        self.app.activate();

        timer.reset();

        let mut frame_count = 0;
        let mut elapsed_time: f64 = 0.0;

        let mut is_running = true;
        let mut is_paused = false;
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
                    //info!("Escape key pressed, exiting.");
                    is_running = false;
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    window_id,
                } if window_id == window.id() => {
                    //info!("Window was closed, exiting.");
                    is_running = false;
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(LogicalSize { width, height }),
                    ..
                } => {
                    //info!("Window size has changed.");
                    renderer.on_window_resized(width as _, height as _);
                }
                Event::Suspended(suspended) => {
                    if suspended {
                        is_paused = true;
                        timer.stop();
                    } else {
                        is_paused = false;
                        timer.start();
                    }
                }
                _ => (),
            });

            timer.tick();

            if !is_paused {
                #[cfg(debug_assertions)]
                {
                    // Calculate FPS
                    frame_count += 1;
                    if timer.total_time() - elapsed_time >= 1.0 {
                        let fps = frame_count;
                        let frame_time = 1000.0 / fps as f32;
                        window.set_title(&format!(
                            "{} [FPS {} - {:.2}ms]",
                            params.window_title, fps, frame_time
                        ));

                        frame_count = 0;
                        elapsed_time += 1.0;
                    }
                }

                self.app.update(timer);

                renderer.prepare();
                renderer.clear();

                self.app.render(timer);

                renderer.present();
            }
        }

        self.app.deactivate();
    }
}
