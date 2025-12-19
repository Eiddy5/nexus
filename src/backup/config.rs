use std::sync::Arc;
use std::time::Duration;
use crate::backup::types::Extension;

#[derive(Clone)]
pub struct Configuration {
	pub name: Option<String>,
	pub timeout: Duration,
	pub debounce: Duration,
	pub max_debounce: Duration,
	pub quiet: bool,
	pub unload_immediately: bool,
	pub extensions: Vec<Arc<dyn Extension>>,
}


impl Default for Configuration {
	fn default() -> Self {
		default_configuration()
	}
}

pub fn default_configuration() -> Configuration {
	Configuration {
		name: None,
		timeout: Duration::from_secs(30),
		debounce: Duration::from_millis(2000),
		max_debounce: Duration::from_millis(10_000),
		quiet: false,
		unload_immediately: true,
		extensions: Vec::new(),
	}
}

#[derive(Clone)]
pub struct ServerConfiguration {
	pub port: u16,
	pub address: String,
	pub stop_on_signals: bool,
	pub inner: Configuration,
}

impl Default for ServerConfiguration {
	fn default() -> Self {
		default_server_configuration()
	}
}

pub fn default_server_configuration() -> ServerConfiguration {
	ServerConfiguration {
		port: 80,
		address: "0.0.0.0".into(),
		stop_on_signals: true,
		inner: default_configuration(),
	}
}

