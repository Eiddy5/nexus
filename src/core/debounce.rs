use std::collections::HashMap;
use std::future::Future;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub struct Debouncer {
	timers: Mutex<HashMap<String, (Instant, tokio::task::JoinHandle<()>)>>,
}

impl Debouncer {
	pub fn new() -> Self {
		Self {
			timers: Mutex::new(HashMap::new()),
		}
	}

	pub async fn debounce<F, Fut>(
		&self,
		id: impl Into<String>,
		delay: Duration,
		max_delay: Duration,
		f: F,
	) where
		F: FnOnce() -> Fut + Send + 'static,
		Fut: Future<Output = ()> + Send + 'static,
	{
		let id = id.into();
		let mut timers = self.timers.lock().await;
		if let Some((start, handle)) = timers.remove(&id) {
			if start.elapsed() < max_delay {
				handle.abort();
			}
		}

		let start = Instant::now();
		let handle = tokio::spawn(async move {
			if delay.is_zero() {
				f().await;
				return;
			}
			tokio::time::sleep(delay).await;
			f().await;
		});

		timers.insert(id, (start, handle));
	}
}

