use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServerError {
	#[error("invalid message: {0}")]
	InvalidMessage(String),
	#[error("sync error: {0}")]
	Sync(String),
	#[error(transparent)]
	Other(#[from] anyhow::Error),
	#[error(transparent)]
	Yrs(#[from] yrs::error::Error),
}

