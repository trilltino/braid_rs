//! Server-side subscription utilities for Braid protocol.

use axum::body::Bytes;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::time::{interval, Duration, Interval};

/// A stream wrapper that injects heartbeat blank lines into a Braid subscription.
///
/// Matches the Braid-HTTP specification (Section 4.1), which recommends sending
/// blank lines (\r\n) at regular intervals to prevent intermediate proxies from
/// timing out the connection.
pub struct HeartbeatStream<S> {
    /// The underlying update stream
    inner: S,
    /// Interval for heartbeats
    heartbeat: Interval,
    /// Whether the last yielded item was a heartbeat
    pending_heartbeat: bool,
}

impl<S> HeartbeatStream<S> {
    /// Create a new heartbeat stream.
    ///
    /// # Arguments
    ///
    /// * `inner` - The stream of updates/data to wrap
    /// * `delay` - The interval between heartbeats
    pub fn new(inner: S, delay: Duration) -> Self {
        let mut heartbeat = interval(delay);
        // The first tick happens immediately, we skip it
        heartbeat.reset();

        Self {
            inner,
            heartbeat,
            pending_heartbeat: false,
        }
    }
}

impl<S, T, E> Stream for HeartbeatStream<S>
where
    S: Stream<Item = Result<T, E>> + Unpin,
    T: From<Bytes>,
{
    type Item = Result<T, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // 1. Check if the inner stream has data
        match self.inner.poll_next_unpin(cx) {
            Poll::Ready(Some(item)) => {
                // We got data, reset the heartbeat timer
                self.heartbeat.reset();
                return Poll::Ready(Some(item));
            }
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => {}
        }

        // 2. Check if it's time for a heartbeat
        match self.heartbeat.poll_tick(cx) {
            Poll::Ready(_) => {
                // It's time! Yield a blank line (\r\n)
                // In Braid, blank lines are ignored as whitespace but keep the connection alive
                Poll::Ready(Some(Ok(T::from(Bytes::from("\r\n")))))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[tokio::test]
    async fn test_heartbeat_injection() {
        let data = vec![Ok::<Bytes, std::io::Error>(Bytes::from("data"))];
        let inner = stream::iter(data);

        // Use a very short interval for testing
        let mut hb_stream = HeartbeatStream::new(inner, Duration::from_millis(10));

        // 1. First item should be "data"
        let first = hb_stream.next().await.unwrap().unwrap();
        assert_eq!(first, Bytes::from("data"));

        // 2. Wait for heartbeat
        tokio::time::sleep(Duration::from_millis(15)).await;
        let second = hb_stream.next().await.unwrap().unwrap();
        assert_eq!(second, Bytes::from("\r\n"));
    }
}
