use std::time::Duration;

#[derive(Debug,Clone)]
pub struct NexusSetting {
    pub timeout: Duration,
    pub debounce: Duration,
    pub max_debounce: Duration,
    pub quiet: bool,
    pub unload_immediately: bool,
}

impl Default for NexusSetting {
    fn default() -> Self {
        default_setting()
    }
}

pub fn default_setting() -> NexusSetting {
    NexusSetting {
        timeout: Duration::from_secs(30),
        debounce: Duration::from_millis(2000),
        max_debounce: Duration::from_millis(10_000),
        quiet: false,
        unload_immediately: true,
    }
}
