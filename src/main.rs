use adamant;

use env_logger::{self, Env};

use log::info;

use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowExtWindows;

fn main() {
    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "trace")
        .write_style_or("MY_LOG_STYLE", "auto");

    env_logger::init_from_env(env);

    const WINDOW_WIDTH: u32 = 1280;
    const WINDOW_HEIGHT: u32 = 720;
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(LogicalSize::new(
            f64::from(WINDOW_WIDTH),
            f64::from(WINDOW_HEIGHT),
        ))
        .with_title("Adamant Window")
        .build(&event_loop)
        .unwrap();

    let mut init_params = adamant::InitParams::new(window.hwnd() as *mut _, 1280, 720);
    init_params.flags = adamant::InitFlags::ALLOW_TEARING | adamant::InitFlags::ENABLE_HDR;
    adamant::init_d3d12(init_params);

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::EventsCleared => {
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                // Redraw the application.
                //
                // It's preferrable to render in this event rather than in EventsCleared, since
                // rendering in here allows the program to gracefully handle redraws requested
                // by the OS.
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                info!("The close button was pressed; stopping");
                *control_flow = ControlFlow::Exit
            }
            // ControlFlow::Poll continuously runs the event loop, even if the OS hasn't
            // dispatched any events. This is ideal for games and similar applications.
            _ => *control_flow = ControlFlow::Poll,
            // ControlFlow::Wait pauses the event loop if no events are available to process.
            // This is ideal for non-game applications that only update in response to user
            // input, and uses significantly less power/CPU time than ControlFlow::Poll.
            // _ => *control_flow = ControlFlow::Wait,
        }
    });
}
