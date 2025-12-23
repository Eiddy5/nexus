use tokio::select;
use tokio::task::JoinHandle;
use yrs::sync::Error;

#[derive(Debug)]
pub struct Channel {
    sink_task: JoinHandle<Result<(), Error>>,
    stream_task: JoinHandle<Result<(), Error>>,
}

impl Channel {
    pub fn new(
        sink_task: JoinHandle<Result<(), Error>>,
        stream_task: JoinHandle<Result<(), Error>>,
    ) -> Self {
        Self {
            sink_task,
            stream_task,
        }
    }
    pub async fn completed(self) -> Result<(), Error> {
        let res = select! {
            r1 = self.sink_task => r1,
            r2 = self.stream_task => r2,
        };
        res.map_err(|e| Error::Other(e.into()))?
    }
}
