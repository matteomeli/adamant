extern crate adamant;

use adamant::game_core::{GameApp, GameCore};
use adamant::game_timer::GameTimer;
use adamant::{InitFlags, InitParams};

struct HelloWorld {
    frame_counter: u64,
    elapsed_time_secs: f64,
}

impl HelloWorld {
    pub fn new() -> Self {
        HelloWorld {
            frame_counter: 0,
            elapsed_time_secs: 0.0,
        }
    }
}

impl GameApp for HelloWorld {
    fn startup(&mut self) {}
    fn cleanup(&mut self) {}
    fn update(&mut self, timer: &GameTimer) {
        // Calculate FPS
        self.frame_counter += 1;
        self.elapsed_time_secs += timer.delta_time();
        if self.elapsed_time_secs > 1.0 {
            let fps = self.frame_counter as f64 / self.elapsed_time_secs;
            println!("FPS: {}", fps);

            self.frame_counter = 0;
            self.elapsed_time_secs = 0.0;
        }
    }
    fn render(&self, _timer: &GameTimer) {}
}

fn main() {
    const DISPLAY_WIDTH: u32 = 1280;
    const DISPLAY_HEIGHT: u32 = 720;
    let mut params = InitParams::new(DISPLAY_WIDTH, DISPLAY_HEIGHT);
    params.flags = InitFlags::ALLOW_TEARING | InitFlags::ENABLE_HDR;

    let mut app = HelloWorld::new();

    GameCore::run(&mut app, params);
}
