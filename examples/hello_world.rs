extern crate adamant;

use adamant::game_core::{GameApp, GameCore};
use adamant::game_timer::GameTimer;
use adamant::{InitFlags, InitParams};

struct HelloWorld {
    params: InitParams,
}

impl HelloWorld {
    pub fn new() -> Self {
        const DISPLAY_WIDTH: u32 = 1280;
        const DISPLAY_HEIGHT: u32 = 720;
        let mut params = InitParams::new("Hello World".to_string(), DISPLAY_WIDTH, DISPLAY_HEIGHT);
        params.flags = InitFlags::ALLOW_TEARING | InitFlags::ENABLE_HDR;
        HelloWorld { params }
    }
}

impl GameApp for HelloWorld {
    fn get_params(&self) -> InitParams {
        self.params.clone()
    }
    fn activate(&mut self) {}
    fn deactivate(&mut self) {}
    fn update(&mut self, _timer: &GameTimer) {}
    fn render(&self, _timer: &GameTimer) {}
}

fn main() {
    let app = HelloWorld::new();
    let game_core = GameCore::new(app);
    game_core.run();
}
