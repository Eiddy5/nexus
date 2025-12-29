use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::debug;

pub struct Debouncer {
	timers: Arc<Mutex<HashMap<String, (Instant, tokio::task::JoinHandle<()>)>>>,
}

impl Debouncer {
	pub fn new() -> Self {
		Self {
			timers: Arc::new(Mutex::new(HashMap::new())),
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
		
		let (should_execute_immediately, original_start) = if let Some((start, handle)) = timers.remove(&id) {
			let elapsed = start.elapsed();
			if elapsed >= max_delay {
				handle.abort();
				(true, start)
			} else {
				handle.abort();
				(false, start) // 关键：保留原始的 start 时间
			}
		} else {
			(false, Instant::now())
		};
	
		if should_execute_immediately {
			drop(timers);
			f().await;
		} else {
			let timers_clone = self.timers.clone();
			let id_clone = id.clone();
			let handle = tokio::spawn(async move {
				if !delay.is_zero() {
					tokio::time::sleep(delay).await;
				}
				f().await;
				let mut timers = timers_clone.lock().await;
				timers.remove(&id_clone);
			});
	
			timers.insert(id, (original_start, handle));
		}
	}
}

