use std::time::{Duration, Instant};

pub struct GameTimer {
    base_time: Instant,
    current_time: Instant,
    previous_time: Instant,
    stop_time: Instant,
    delta_time: Duration,
    paused_time: Duration,
    is_stopped: bool,
    pub total_frames: u64,
}

impl GameTimer {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn delta_time(&self) -> f64 {
        self.delta_time.as_secs_f64()
    }

    pub fn total_time(&self) -> f64 {
        let total_time = if self.is_stopped {
            (self.stop_time - self.base_time) - self.paused_time
        } else {
            (self.current_time - self.base_time) - self.paused_time
        };
        total_time.as_secs_f64()
    }

    pub fn reset(&mut self) {
        self.base_time = Instant::now();
        self.previous_time = Instant::now();
        self.stop_time = Instant::now();
        self.is_stopped = false;
    }

    pub fn start(&mut self) {
        if self.is_stopped {
            let now = Instant::now();
            self.paused_time += now - self.stop_time;
            self.stop_time = now;
            self.is_stopped = false;
        }
    }

    pub fn stop(&mut self) {
        if !self.is_stopped {
            let now = Instant::now();
            self.stop_time = now;
            self.is_stopped = true;
        }
    }

    pub fn tick(&mut self) {
        if self.is_stopped {
            self.delta_time = Duration::from_secs_f64(0.0);
            return;
        }

        // Update delta time for last frame
        self.current_time = Instant::now();
        self.delta_time = self.current_time - self.previous_time;
        self.previous_time = self.current_time;
        self.total_frames += 1;
    }
}

impl Default for GameTimer {
    fn default() -> Self {
        let now = Instant::now();
        let zero_duration = Duration::from_secs_f64(0.0);
        Self {
            base_time: now,
            current_time: now,
            previous_time: now,
            stop_time: now,
            delta_time: zero_duration,
            paused_time: zero_duration,
            is_stopped: false,
            total_frames: 0,
        }
    }
}
