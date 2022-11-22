//! Module for checksum calculation.

use std::marker::PhantomData;
use std::task::Poll;

use crc32fast::Hasher;
use futures::prelude::*;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::Context;

pin_project! {
#[must_use = "sinks do nothing unless polled"]
/// A [Sink] that will calculate the CRC32 of any
/// `AsRef<[u8]>`.
///
/// ```rust
/// # use cobalt_async::checksum::CRC32Sink;
/// # use futures::sink::SinkExt;
/// # use tokio_test;
/// # tokio_test::block_on(async {
///      let mut sink = CRC32Sink::default();
///      sink.send("this is a test".as_bytes()).await?;
///      sink.close().await.unwrap();
///      assert_eq!(220129258, sink.value().unwrap());
///      # Ok::<(), anyhow::Error>(())
/// # });
///
/// ```
/// Attempting to get the [value](`CRC32Sink::value`) of the [Sink] before
/// calling [Sink::poll_close] results in a [None] being returned
/// ```rust
/// # use cobalt_async::checksum::CRC32Sink;
/// # use futures::sink::SinkExt;
/// # use tokio_test;
/// # tokio_test::block_on(async {
///      let mut sink = CRC32Sink::default();
///      sink.send("this is a test".as_bytes()).await?;
///      assert!(sink.value().is_none());
///      # Ok::<(), anyhow::Error>(())
/// # });
///
pub struct CRC32Sink<Item: AsRef<[u8]>> {
    digest: Option<Hasher>,
    value: Option<u32>,
    //Needed to allow StreamExt to determine the type of Item
    marker:  std::marker::PhantomData<Item>
}
}

impl<Item: AsRef<[u8]>> CRC32Sink<Item> {
    /// Produces a new CRC32Sink.
    pub fn new() -> CRC32Sink<Item> {
        CRC32Sink {
            digest: Some(Hasher::new()),
            value: None,
            marker: PhantomData,
        }
    }

    /// Returns the crc32 of the values passed into the
    /// [Sink] once the [Sink] has been closed.
    pub fn value(&self) -> Option<u32> {
        self.value
    }
}

impl<Item: AsRef<[u8]>> Default for CRC32Sink<Item> {
    fn default() -> Self {
        Self::new()
    }
}

/// The [futures] crate provides a [into_sink](futures::io::AsyncWriteExt::into_sink) for AsyncWrite but it is
/// not possible to get the value out of it afterwards as it takes ownership.
impl<Item: AsRef<[u8]>> futures::sink::Sink<Item> for CRC32Sink<Item> {
    type Error = anyhow::Error;

    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        match self.digest {
            Some(_) => Poll::Ready(Ok(())),
            None => Poll::Ready(Err(anyhow::Error::msg("Close has been called."))),
        }
    }

    fn start_send(
        self: Pin<&mut CRC32Sink<Item>>,
        item: Item,
    ) -> std::result::Result<(), Self::Error> {
        let mut this = self.project();
        match &mut this.digest {
            Some(digest) => {
                digest.update(item.as_ref());
                Ok(())
            }
            None => Err(anyhow::Error::msg("Close has been called.")),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        match self.digest {
            Some(_) => Poll::Ready(Ok(())),
            None => Poll::Ready(Err(anyhow::Error::msg("Close has been called."))),
        }
    }

    fn poll_close(
        mut self: Pin<&mut CRC32Sink<Item>>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        match std::mem::take(&mut self.digest) {
            Some(digest) => {
                self.value = Some(digest.finalize());
                Poll::Ready(Ok(()))
            }
            None => Poll::Ready(Err(anyhow::Error::msg("Close has been called."))),
        }
    }
}

impl AsyncWrite for CRC32Sink<&[u8]> {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut this = self.project();
        match &mut this.digest {
            Some(digest) => {
                digest.update(buf);
                Poll::Ready(Ok(buf.len()))
            }
            None => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Close has been called",
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let mut this = self.project();
        match &mut this.digest {
            Some(_) => Poll::Ready(Ok(())),
            None => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Close has been called",
            ))),
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match std::mem::take(&mut self.digest) {
            Some(digest) => {
                self.value = Some(digest.finalize());
                Poll::Ready(Ok(()))
            }
            None => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Close has been called",
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crc32() {
        let mut sink = CRC32Sink::default();
        sink.send(&[0, 100]).await.unwrap();
        sink.close().await.unwrap();
        assert_eq!(184989630, sink.value.unwrap());
    }

    #[tokio::test]
    async fn test_crc32_write_after_close() {
        let mut sink = CRC32Sink::default();
        sink.send(&[0, 100]).await.unwrap();
        sink.close().await.unwrap();
        assert!(sink.send(&[0, 100]).await.is_err());
    }

    #[tokio::test]
    async fn test_crc32_flush_after_close() {
        let mut sink = CRC32Sink::default();
        sink.send(&[0, 100]).await.unwrap();
        sink.close().await.unwrap();
        assert!(sink.flush().await.is_err());
    }

    #[tokio::test]
    async fn test_crc32_close_after_close() {
        let mut sink = CRC32Sink::<&[u8]>::default();
        futures::SinkExt::close(&mut sink).await.unwrap();
        assert!(futures::SinkExt::close(&mut sink).await.is_err());
    }
}
