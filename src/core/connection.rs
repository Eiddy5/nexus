use axum::extract::ws::{Message, WebSocket};
use futures_util::Stream;
use futures_util::stream::{SplitSink, SplitStream};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::select;
use tokio::task::JoinHandle;
use tracing::info;
use yrs::sync::Error;

pub struct Connection {
    doc_id:String,
    sink_task: JoinHandle<Result<(), Error>>,
    stream_task: JoinHandle<Result<(), Error>>,
}

impl Connection {
    pub fn new(
        doc_id:String,
        sink_task: JoinHandle<Result<(), Error>>,
        stream_task: JoinHandle<Result<(), Error>>,
    ) -> Self {
        Self {
            doc_id,
            sink_task,
            stream_task,
        }
    }

    pub async fn completed(self)-> Result<(), Error>{
        let res = select! {
            r1 = self.sink_task => r1,
            r2 = self.stream_task => r2,
        };
        info!("connection completed: {:?}",self.doc_id);
        res.map_err(|e| Error::Other(e.into()))?
    }

}

#[repr(transparent)]
#[derive(Debug)]
pub struct AxumSink(SplitSink<WebSocket, Message>);

impl From<SplitSink<WebSocket, Message>> for AxumSink {
    fn from(sink: SplitSink<WebSocket, Message>) -> Self {
        AxumSink(sink)
    }
}

impl Into<SplitSink<WebSocket, Message>> for AxumSink {
    fn into(self) -> SplitSink<WebSocket, Message> {
        self.0
    }
}

impl futures_util::Sink<Vec<u8>> for AxumSink {
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match Pin::new(&mut self.0).poll_ready(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(Error::Other(e.into()))),
            Poll::Ready(_) => Poll::Ready(Ok(())),
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), Self::Error> {
        if let Err(e) = Pin::new(&mut self.0).start_send(Message::binary(item)) {
            Err(Error::Other(e.into()))
        } else {
            Ok(())
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match Pin::new(&mut self.0).poll_flush(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(Error::Other(e.into()))),
            Poll::Ready(_) => Poll::Ready(Ok(())),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match Pin::new(&mut self.0).poll_close(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(Error::Other(e.into()))),
            Poll::Ready(_) => Poll::Ready(Ok(())),
        }
    }
}

#[derive(Debug)]
pub struct AxumStream(SplitStream<WebSocket>);

impl From<SplitStream<WebSocket>> for AxumStream {
    fn from(stream: SplitStream<WebSocket>) -> Self {
        AxumStream(stream)
    }
}

impl Into<SplitStream<WebSocket>> for AxumStream {
    fn into(self) -> SplitStream<WebSocket> {
        self.0
    }
}

impl Stream for AxumStream {
    type Item = Result<Vec<u8>, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.0).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(res)) => match res {
                Ok(Message::Close(_))=>{
                    Poll::Ready(None)
                }
                Ok(item) =>Poll::Ready(Some(Ok(item.into()))),
                Err(e)=>Poll::Ready(Some(Err(Error::Other(e.into()))))

            },
        }
    }
}
