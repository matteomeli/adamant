extern crate adamant;

use adamant::{Context, ContextFlags, ContextParams, GameTimer};

use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    let params = ContextParams::new(
        "Tutorial 01".to_string(),
        1280,
        720,
        ContextFlags::ALLOW_TEARING,
    );

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_min_inner_size(LogicalSize::new(1.0, 1.0))
        .with_inner_size(LogicalSize::new(
            f64::from(params.window_width),
            f64::from(params.window_height),
        ))
        .with_title(&params.window_title)
        .build(&event_loop)
        .unwrap();

    let mut context = Context::new(&window, &params);

    let mut frame_count = 0;
    let mut elapsed_time: f64 = 0.0;

    let mut timer = GameTimer::new();
    timer.reset();

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::EventsCleared => {
                // Application update code.
                timer.tick();
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

                // Queue a RedrawRequested event.
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                // Redraw the application.
                context.prepare();
                context.clear();
                context.present();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(LogicalSize { width, height }),
                ..
            } => context.on_window_resized(width as _, height as _),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => *control_flow = ControlFlow::Exit,
            _ => *control_flow = ControlFlow::Poll,
        }
    });
}
