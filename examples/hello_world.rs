extern crate adamant;

use adamant::game_core::{GameApp, GameCore};
use adamant::timer::Timer;

struct HelloWorld {}

impl HelloWorld {
    pub fn new() -> Self {
        HelloWorld {}
    }
}

impl GameApp for HelloWorld {
    fn startup(&self) {}
    fn cleanup(&self) {}
    fn is_done(&self) -> bool {
        return false;
    }
    fn update(&self, timer: &Timer) {
        // Just print the framerate to console
        print!("FPS: {}", timer.frames_per_second);
        print!("\r");
    }
    fn render(&self) {}
}

fn main() {
    GameCore::run(HelloWorld::new());
}
