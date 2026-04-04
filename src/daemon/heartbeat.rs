use std::time::Duration;

#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    pub interval: Duration,
    pub min_interval: Duration,
    pub max_interval: Duration,
    pub adaptive: bool,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            min_interval: Duration::from_secs(5),
            max_interval: Duration::from_secs(120),
            adaptive: true,
        }
    }
}

pub struct Heartbeat {
    config: HeartbeatConfig,
    current_interval: Duration,
    tick_count: u64,
}

impl Heartbeat {
    pub fn new(config: HeartbeatConfig) -> Self {
        let interval = config.interval;
        Self {
            config,
            current_interval: interval,
            tick_count: 0,
        }
    }

    pub async fn wait_tick(&mut self) {
        tokio::time::sleep(self.current_interval).await;
        self.tick_count += 1;
    }

    pub fn report_signals(&mut self, count: usize) {
        if !self.config.adaptive { return; }
        if count > 0 {
            self.current_interval = self.current_interval.checked_div(2)
                .unwrap_or(self.config.min_interval)
                .max(self.config.min_interval);
        } else {
            self.current_interval = (self.current_interval + Duration::from_secs(5))
                .min(self.config.max_interval);
        }
    }

    pub fn reset_interval(&mut self) {
        self.current_interval = self.config.interval;
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }
}
