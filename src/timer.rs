use crate::game_core::GameApp;

use std::convert::TryInto;
use std::time::{Duration, Instant};

pub struct Timer {
    previous_time: Instant,
    max_delta_time_us: u128,         // Max delta time per tick (microseconds)
    target_time_per_update_us: u128, // Target time per update in fixed timestep logic (microseconds)
    lag_time_us: u128,               // Time to divide in the fixed timestep logic (microseconds)
    total_time_us: u128,             // Total time since the start of the program (microseconds)
    frame_elapsed_time: u128,        // Time elapsed since the previous update (microseconds)
    frame_elapsed_time_to_second_us: u128, // Current elapsed time since the last FPS measurement (microseconds)
    frame_count_to_second: u64,            // Current count of frames for the next FPS measurement
    pub frames_per_second: u64,            // Frames per second
    pub frame_count: u64,                  // Total frame count
    is_fixed_time_step: bool,              // Should we use fixed or variable timestep
}

impl Timer {
    pub fn new() -> Self {
        Self {
            previous_time: Instant::now(),
            max_delta_time_us: 100000,        // 1/10 of a second
            target_time_per_update_us: 16667, // 60 frames per second = 16.6667 = 16666.7 ~ 16667
            lag_time_us: 0,
            total_time_us: 0,
            frame_elapsed_time: 0,
            frame_elapsed_time_to_second_us: 0,
            frame_count_to_second: 0,
            frames_per_second: 0,
            frame_count: 0,
            is_fixed_time_step: false,
        }
    }

    pub fn elapsed_time_secs(&self) -> f64 {
        f64::from(self.frame_elapsed_time as u32) * 1e-6
    }

    pub fn total_seconds(&self) -> f64 {
        f64::from(self.total_time_us as u32) * 1e-6
    }

    pub fn update(&mut self, app: &impl GameApp) {
        let now = Instant::now();
        let delta_time = now - self.previous_time;
        let mut delta_time_us = delta_time.as_micros();
        self.previous_time = now;
        self.frame_elapsed_time_to_second_us += delta_time_us;

        // Clamp excessively large time deltas (e.g. after paused in debugger)
        if delta_time_us > self.max_delta_time_us {
            delta_time_us = self.max_delta_time_us;
        }

        let previous_frame_count = self.frame_count;

        if self.is_fixed_time_step {
            // Fixed timestep update logic

            // If running very close to the target elapse time (1/4 of a millisecond), clamp the elapsed time
            if i128::abs((self.lag_time_us - self.target_time_per_update_us) as i128) < 250 {
                delta_time_us = self.target_time_per_update_us;
            }

            self.lag_time_us += delta_time_us;

            while self.lag_time_us >= self.target_time_per_update_us {
                self.frame_elapsed_time = self.target_time_per_update_us;
                self.total_time_us += self.target_time_per_update_us;
                self.lag_time_us -= self.target_time_per_update_us;
                self.frame_count += 1;

                app.update(&self);
            }
        } else {
            // Variable timestep update logic
            self.frame_elapsed_time = delta_time_us;
            self.total_time_us = delta_time_us;
            self.lag_time_us = 0;
            self.frame_count += 1;

            app.update(&self);
        }

        // Track current framerate
        if self.frame_count != previous_frame_count {
            self.frame_count_to_second += 1;
        }

        // If a second as passed since last measurement, record the FPS and reset
        if Duration::from_micros(self.frame_elapsed_time_to_second_us.try_into().unwrap())
            .as_secs_f64()
            > 1.0
        {
            self.frames_per_second = self.frame_count_to_second;
            self.frame_count_to_second = 0;
            self.frame_elapsed_time_to_second_us = 0;
        }
    }
}
