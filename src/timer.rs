use std::time::{Duration, Instant};

use anyhow::Result;
use cancellable_timer::{Canceller, Timer};
use tokio::task::spawn;
use tokio::time::sleep_until;

/// A sleep timer that can cancel sleep at any time and observe remaining time periodically.
pub struct ObservableTimer {
    timer: Timer,
}

impl ObservableTimer {
    pub fn new() -> Result<(Self, Canceller)> {
        let (timer, canceller) = Timer::new2()?;
        Ok((Self { timer }, canceller))
    }

    pub async fn sleep<F>(
        mut self,
        total_duration: Duration,
        inspection_interval: Duration,
        mut inspect: F,
    ) -> Result<()>
    where
        F: FnMut(Duration),
    {
        let start = Instant::now();
        let entire_sleep = spawn(async move { self.timer.sleep(total_duration) });
        tokio::pin!(entire_sleep);

        let mut next_inspection = start;
        loop {
            let inspection_sleep = sleep_until(next_inspection.into());

            tokio::select! {
                end = &mut entire_sleep => return Ok(end??),
                _ = inspection_sleep => {
                    let elapsed = start.elapsed();
                    if total_duration > elapsed {
                        let remaining = total_duration - elapsed;
                        inspect(remaining)
                    }
                },
            }

            next_inspection += inspection_interval;
        }
    }
}
