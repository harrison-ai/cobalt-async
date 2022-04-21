//! # The Cobalt Async wrapper library
//!
//! This library provides a collection of helpful functions for working with async Rust.
//!
//! * [Changelog](https://github.com/harrison-ai/cobalt-async/blob/main/CHANGELOG.md)
//!
//! ### About harrison.ai
//!
//! This crate is maintained by the Data Engineering team at [harrison.ai](https://harrison.ai).
//!
//! At [harrison.ai](https://harrison.ai) our mission is to create AI-as-a-medical-device solutions through
//! ventures and ultimately improve the standard of healthcare for 1 million lives every day.
//!

use futures::{stream, Future, Stream, StreamExt, TryStream, TryStreamExt};

/// The return value of chunker functions used with [`apply_chunker`].
pub enum ChunkResult<Chunk, State> {
    /// Indicates that `apply_chunker` should continue to the next input stream value without yielding a chunk.
    Continue(
        /// The updated chunk value to be passed to the next iteration.
        Option<Chunk>,
        /// The updated state value to be passed to the next iteration.
        State,
    ),
    /// Indicates that `apply_chunker` should yield a chunk and then continue to the next input stream value.
    Yield(
        /// The updated chunk value to be passed to the next iteration.
        Option<Chunk>,
        /// The updated state value to be passed to the next iteration.
        State,
        /// The chunk to be yielded to the output stream.
        Chunk,
    ),
}

/// Converts a [`Stream`] of type `T` to a `Stream` of type `Chunk` by applying the function `chunker`
/// to each item in the stream in turn.
///
/// On each call `chunker` can either return [`ChunkResult::Yield`] to yield a chunk to the output stream,
/// or [`ChunkResult::Continue`] to continue on to the next item without yielding.
///
/// The `chunker` is passed three arguments; the next item in the input stream (`T`), the current chunk (`Option<Chunk>`), and the current state (`State`).
/// On each call, the chunker can update the current chunk and the current state.
/// The chunker must return the updated chunk and state values.
///
/// Once the input stream has exhausted, if the current chunk is not `None` it will be automatically yielded as the final value in the output stream.
///
/// The caller must provide initial values for both the chunk and state.
///
/// # Example
///
/// ```
/// use cobalt_async::{apply_chunker, ChunkResult};
/// use futures::{stream, StreamExt};
///
/// /// Takes an input stream of numerical values and returns a stream of vectors of numbers,
/// /// where the sum of each vector no more than ten.
/// /// If a value greater than ten is encountered, it is yielded as a vector with a single value.
/// async fn ten_chunker(
///     val: u64,
///     chunk: Option<Vec<u64>>,
///     state: u64,  // The current running sum
/// ) -> ChunkResult<Vec<u64>, u64> {
///     if state + val > 10 {
///         // Yield the current chunk, and start a new chunk
///         ChunkResult::Yield(Some(vec![val]), val, chunk.unwrap())
///     } else {
///         // Add the value to the current chunk, and update the sum
///         let mut chunk = chunk.unwrap_or_default();
///         chunk.push(val);
///         ChunkResult::Continue(Some(chunk), state + val)
///     }
/// }
///
/// # tokio_test::block_on(async {
/// let stream = stream::iter(vec![1, 2, 3, 4, 5, 6, 3, 13, 4, 5]);
/// let groups = apply_chunker(ten_chunker, stream, None, 0).collect::<Vec<_>>().await;
/// assert_eq!(groups, vec![vec![1, 2, 3, 4], vec![5], vec![6, 3], vec![13], vec![4, 5]]);
/// # })
/// ```
pub fn apply_chunker<T, Chunk, State, F, Fut>(
    chunker: F,
    stream: impl Stream<Item = T> + Unpin,
    initial_chunk: Option<Chunk>,
    initial_state: State,
) -> impl Stream<Item = Chunk> + Unpin
where
    F: Fn(T, Option<Chunk>, State) -> Fut,
    Fut: Future<Output = ChunkResult<Chunk, State>>,
{
    let complete = false;
    Box::pin(stream::unfold(
        (initial_chunk, initial_state, stream, chunker, complete),
        |(mut current_chunk, mut current_state, mut stream, chunker, complete)| async move {
            if complete {
                return None;
            }

            // Iterate over the stream, and pass each item to the chunking function
            while let Some(item) = stream.next().await {
                match chunker(item, current_chunk, current_state).await {
                    ChunkResult::Continue(chunk, state) => {
                        current_chunk = chunk;
                        current_state = state;
                    }
                    ChunkResult::Yield(chunk, state, complete_chunk) => {
                        return Some((complete_chunk, (chunk, state, stream, chunker, false)));
                    }
                }
            }

            // writing this as current_chunk.map(...) makes the logic less obvious
            #[allow(clippy::manual_map)]
            // The stream ran out, so check if we have a pending chunk
            match current_chunk {
                Some(chunk) => {
                    // If we do, yield the pending chunk, and set the "complete"
                    // flag to true, so that we can yield a final None next time around the loop.
                    Some((chunk, (None, current_state, stream, chunker, true)))
                }
                // Otherwise simply return None.
                None => None,
            }
        },
    ))
}

/// Converts a [`TryStream`] of type `T` to a `TryStream` of type `Chunk` by applying the function `chunker`
/// to each item in the stream in turn.
///
/// On each call `chunker` can either return [`ChunkResult::Yield`] to yield a chunk to the output stream,
/// or [`ChunkResult::Continue`] to continue on to the next item without yielding.
///
/// The `chunker` is passed three arguments; the next item in the input stream (`T`), the current chunk (`Option<Chunk>`), and the current state (`State`).
/// On each call, the chunker can update the current chunk and the current state.
/// The chunker must return the updated chunk and state values.
///
/// Once the input stream has exhausted, if the current chunk is not `None` it will be automatically yielded as the final value in the output stream.
///
/// In an `Err` is encountered in the input stream, it is passed through to the output stream while maintaining the current chunk and state.
///
/// The caller must provide initial values for both the chunk and state.
///
/// # Example
///
/// ```
/// use cobalt_async::{try_apply_chunker, ChunkResult};
/// use futures::{stream, StreamExt, TryStreamExt};
///
/// /// Takes an input stream of numerical values and returns a stream of vectors of numbers,
/// /// where the sum of each vector no more than ten.
/// /// If a value greater than ten is encountered, it is yielded as a vector with a single value.
/// async fn ten_chunker(
///     val: u64,
///     chunk: Option<Vec<u64>>,
///     state: u64,  // The current running sum
/// ) -> ChunkResult<Vec<u64>, u64> {
///     if state + val > 10 {
///         // Yield the current chunk, and start a new chunk
///         ChunkResult::Yield(Some(vec![val]), val, chunk.unwrap())
///     } else {
///         // Add the value to the current chunk, and update the sum
///         let mut chunk = chunk.unwrap_or_default();
///         chunk.push(val);
///         ChunkResult::Continue(Some(chunk), state + val)
///     }
/// }
///
/// # tokio_test::block_on(async {
///
/// // A `TryStream` of values with no errors
/// let stream = stream::iter(vec![Ok::<_,std::io::Error>(1), Ok(2), Ok(3), Ok(4), Ok(5), Ok(6)]);
/// let groups = try_apply_chunker(ten_chunker, stream, None, 0)
///     .try_collect::<Vec<_>>()
///     .await
///     .unwrap();
/// assert_eq!(groups, vec![vec![1, 2, 3, 4], vec![5], vec![6]]);
///
/// // A `TryStream` of values with interleaved errors
/// let stream = stream::iter(vec![
///     Ok(1),
///     Ok(2),
///     Err("Error"),
///     Ok(3),
///     Ok(4),
///     Ok(5),
///     Err("Error"),
///     Ok(6),
/// ]);
/// let groups = try_apply_chunker(ten_chunker, stream, None, 0)
///     .into_stream()
///     .collect::<Vec<_>>()
///     .await;
///
/// // The resulting `TryStream` includes the errors
/// assert!(groups[0].is_err());
/// assert_eq!(*groups[1].as_ref().unwrap(), vec![1, 2, 3, 4]);
/// assert!(groups[2].is_err());
/// assert_eq!(*groups[3].as_ref().unwrap(), vec![5]);
/// assert_eq!(*groups[4].as_ref().unwrap(), vec![6]);
/// # })
/// ```
pub fn try_apply_chunker<T, E, Chunk, State, F, Fut>(
    chunker: F,
    stream: impl TryStream<Ok = T, Error = E> + Unpin,
    initial_chunk: Option<Chunk>,
    initial_state: State,
) -> impl TryStream<Ok = Chunk, Error = E> + Unpin
where
    F: Fn(T, Option<Chunk>, State) -> Fut,
    Fut: Future<Output = ChunkResult<Chunk, State>>,
{
    let complete = false;
    Box::pin(stream::unfold(
        (initial_chunk, initial_state, stream, chunker, complete),
        |(mut current_chunk, mut current_state, mut stream, chunker, complete)| async move {
            if complete {
                return None;
            }
            loop {
                match stream.try_next().await {
                    Err(e) => {
                        // An error in the input stream. Return an error, but maintain the current chunk
                        return Some((
                            Err(e),
                            (current_chunk, current_state, stream, chunker, complete),
                        ));
                    }
                    Ok(None) => {
                        // writing this as current_chunk.map(...) makes the logic less obvious
                        #[allow(clippy::manual_map)]
                        // The stream ran out, so check if we have a pending chunk
                        match current_chunk {
                            Some(chunk) => {
                                // If we do, yield the pending chunk, and set the "complete"
                                // flag to true, so that we can yield a final None next time around the loop.
                                return Some((
                                    Ok(chunk),
                                    (None, current_state, stream, chunker, true),
                                ));
                            }
                            // Otherwise simply return None.
                            None => return None,
                        }
                    }
                    Ok(Some(item)) => match chunker(item, current_chunk, current_state).await {
                        ChunkResult::Continue(chunk, state) => {
                            current_chunk = chunk;
                            current_state = state;
                        }
                        ChunkResult::Yield(chunk, state, complete_chunk) => {
                            return Some((
                                Ok(complete_chunk),
                                (chunk, state, stream, chunker, false),
                            ));
                        }
                    },
                }
            }
        },
    ))
}

#[cfg(test)]
mod test_apply_chunker {

    use super::*;
    use futures::stream::empty;

    /// Yields the values it recieves
    async fn identity_chunker<T>(item: T, _: Option<T>, _: ()) -> ChunkResult<T, ()> {
        ChunkResult::Yield(None, (), item)
    }

    #[tokio::test]
    async fn test_identity() {
        // An empty stream yields an empty stream
        let x = apply_chunker(identity_chunker, empty::<()>(), None, ())
            .collect::<Vec<_>>()
            .await;
        assert_eq!(x, vec![]);

        // A stream with values yields the same values
        let stream = stream::iter(vec![1, 2, 3, 4]);
        let x = apply_chunker(identity_chunker, stream, None, ())
            .collect::<Vec<_>>()
            .await;
        assert_eq!(x, vec![1, 2, 3, 4]);
    }

    /// Never yields any values
    async fn null_chunker<T>(_: T, _: Option<T>, _: ()) -> ChunkResult<T, ()> {
        ChunkResult::Continue(None, ())
    }

    #[tokio::test]
    async fn test_null() {
        // An empty stream yields an empty stream
        let x = apply_chunker(null_chunker, empty::<()>(), None, ())
            .collect::<Vec<_>>()
            .await;
        assert_eq!(x, vec![]);

        // A stream with values yields an empty stream
        let stream = stream::iter(vec![1, 2, 3, 4]);
        let x = apply_chunker(null_chunker, stream, None, ())
            .collect::<Vec<_>>()
            .await;
        assert_eq!(x, vec![]);
    }

    /// Yields pairs of values. If there are an odd number of values, the un-paired value is dropped.
    async fn pair_chunker<T>(
        item: T,
        _: Option<(T, T)>,
        state: Option<T>,
    ) -> ChunkResult<(T, T), Option<T>> {
        match state {
            Some(first) => ChunkResult::Yield(None, None, (first, item)),
            None => ChunkResult::Continue(None, Some(item)),
        }
    }

    #[tokio::test]
    async fn test_pairs() {
        // An empty stream yields an empty stream
        let x = apply_chunker(pair_chunker, empty::<()>(), None, None)
            .collect::<Vec<_>>()
            .await;
        assert_eq!(x, vec![]);

        // A stream with values yields pairs, but drops the last value
        let stream = stream::iter(vec![1, 2, 3, 4, 5]);
        let x = apply_chunker(pair_chunker, stream, None, None)
            .collect::<Vec<_>>()
            .await;
        assert_eq!(x, vec![(1, 2), (3, 4)]);
    }

    /// Yields vectors of values with a sum less than ten (unless the value itself)
    /// is greater than 10, in which case we return a vec with just that value.
    async fn ten_chunker(
        item: u64,
        chunk: Option<Vec<u64>>,
        state: u64,
    ) -> ChunkResult<Vec<u64>, u64> {
        if state + item > 10 {
            ChunkResult::Yield(Some(vec![item]), item, chunk.unwrap())
        } else {
            let mut chunk = chunk.unwrap_or_default();
            chunk.push(item);
            ChunkResult::Continue(Some(chunk), state + item)
        }
    }

    #[tokio::test]
    async fn test_ten_chunker() {
        // An empty stream yields an empty stream
        let x = apply_chunker(ten_chunker, empty::<u64>(), None, 0)
            .collect::<Vec<Vec<u64>>>()
            .await;
        let y: Vec<Vec<_>> = vec![];
        assert_eq!(x, y);

        // A stream with values yields pairs, but drops the last value
        let stream = stream::iter(vec![1, 2, 3, 4, 5, 6, 3, 13, 4, 5]);
        let x = apply_chunker(ten_chunker, stream, None, 0)
            .collect::<Vec<_>>()
            .await;
        assert_eq!(
            x,
            vec![vec![1, 2, 3, 4], vec![5], vec![6, 3], vec![13], vec![4, 5]]
        );
    }
}

#[cfg(test)]
mod test_try_apply_chunker {

    use super::*;
    use anyhow::Result;
    use futures::stream::empty;

    /// Yields the values it recieves
    async fn identity_chunker<T>(item: T, _: Option<T>, _: ()) -> ChunkResult<T, ()> {
        ChunkResult::Yield(None, (), item)
    }

    #[tokio::test]
    async fn test_identity() -> Result<()> {
        // An empty stream yields an empty stream
        let x = try_apply_chunker(identity_chunker, empty::<Result<()>>(), None, ())
            .try_collect::<Vec<_>>()
            .await?;
        assert_eq!(x, vec![]);

        // A stream with values yields the same values
        let stream = stream::iter(vec![Ok::<_, anyhow::Error>(1), Ok(2), Ok(3), Ok(4)]);
        let x = try_apply_chunker(identity_chunker, stream, None, ())
            .try_collect::<Vec<_>>()
            .await?;
        assert_eq!(x, vec![1, 2, 3, 4]);

        // A stream with values and errors yields the same values and errors
        let stream = stream::iter(vec![
            Ok(1),
            Err(anyhow::anyhow!("ERR")),
            Ok(2),
            Ok(3),
            Ok(4),
            Err(anyhow::anyhow!("ERR")),
        ]);
        let x = try_apply_chunker(identity_chunker, stream, None, ())
            .into_stream()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(*x[0].as_ref().unwrap(), 1);
        assert!(x[1].is_err());
        assert_eq!(*x[2].as_ref().unwrap(), 2);
        assert_eq!(*x[3].as_ref().unwrap(), 3);
        assert_eq!(*x[4].as_ref().unwrap(), 4);
        assert!(x[5].is_err());

        Ok(())
    }

    /// Never yields any values
    async fn null_chunker<T>(_: T, _: Option<T>, _: ()) -> ChunkResult<T, ()> {
        ChunkResult::Continue(None, ())
    }

    #[tokio::test]
    async fn test_null() -> Result<()> {
        // An empty stream yields an empty stream
        let x = try_apply_chunker(null_chunker, empty::<Result<()>>(), None, ())
            .try_collect::<Vec<_>>()
            .await?;
        assert_eq!(x, vec![]);

        // A stream with values yields an empty stream
        let stream = stream::iter(vec![Ok::<_, anyhow::Error>(1), Ok(2), Ok(3), Ok(4)]);
        let x = try_apply_chunker(null_chunker, stream, None, ())
            .try_collect::<Vec<_>>()
            .await?;
        assert_eq!(x, vec![]);

        // A stream with values and errors yields just the errors
        let stream = stream::iter(vec![
            Ok(1),
            Err(anyhow::anyhow!("ERR")),
            Ok(2),
            Ok(3),
            Ok(4),
            Err(anyhow::anyhow!("ERR")),
        ]);
        let x = try_apply_chunker(null_chunker, stream, None, ())
            .into_stream()
            .collect::<Vec<_>>()
            .await;
        assert!(x[0].is_err());
        assert!(x[1].is_err());

        Ok(())
    }

    /// Yields pairs of values. If there are an odd number of values, the un-paired value is dropped.
    async fn pair_chunker<T>(
        item: T,
        _: Option<(T, T)>,
        state: Option<T>,
    ) -> ChunkResult<(T, T), Option<T>> {
        match state {
            Some(first) => ChunkResult::Yield(None, None, (first, item)),
            None => ChunkResult::Continue(None, Some(item)),
        }
    }

    #[tokio::test]
    async fn test_pairs() -> Result<()> {
        // An empty stream yields an empty stream
        let x = try_apply_chunker(pair_chunker, empty::<Result<()>>(), None, None)
            .try_collect::<Vec<_>>()
            .await?;
        assert_eq!(x, vec![]);

        // A stream with values yields pairs, but drops the last value
        let stream = stream::iter(vec![Ok::<_, anyhow::Error>(1), Ok(2), Ok(3), Ok(4), Ok(5)]);
        let x = try_apply_chunker(pair_chunker, stream, None, None)
            .try_collect::<Vec<_>>()
            .await?;
        assert_eq!(x, vec![(1, 2), (3, 4)]);

        // A stream with values and errors yields pairs, and interleaves the errors
        let stream = stream::iter(vec![
            Ok(1),
            Err(anyhow::anyhow!("ERR")),
            Ok(2),
            Ok(3),
            Ok(4),
            Err(anyhow::anyhow!("ERR")),
        ]);
        let x = try_apply_chunker(pair_chunker, stream, None, None)
            .into_stream()
            .collect::<Vec<_>>()
            .await;
        assert!(x[0].is_err());
        assert_eq!(*x[1].as_ref().unwrap(), (1, 2));
        assert_eq!(*x[2].as_ref().unwrap(), (3, 4));
        assert!(x[3].is_err());

        Ok(())
    }

    /// Yields vectors of values with a sum less than ten (unless the value itself)
    /// is greater than 10, in which case we return a vec with just that value.
    async fn ten_chunker(
        item: u64,
        chunk: Option<Vec<u64>>,
        state: u64,
    ) -> ChunkResult<Vec<u64>, u64> {
        if state + item > 10 {
            ChunkResult::Yield(Some(vec![item]), item, chunk.unwrap())
        } else {
            let mut chunk = chunk.unwrap_or_default();
            chunk.push(item);
            ChunkResult::Continue(Some(chunk), state + item)
        }
    }

    #[tokio::test]
    async fn test_ten_chunker() -> Result<()> {
        // An empty stream yields an empty stream
        let x = try_apply_chunker(ten_chunker, empty::<Result<u64>>(), None, 0)
            .try_collect::<Vec<Vec<u64>>>()
            .await?;
        let y: Vec<Vec<_>> = vec![];
        assert_eq!(x, y);

        // A stream with values yields groups which add up to ten
        let stream = stream::iter(vec![
            Ok::<_, anyhow::Error>(1),
            Ok(2),
            Ok(3),
            Ok(4),
            Ok(5),
            Ok(6),
            Ok(3),
            Ok(13),
            Ok(4),
            Ok(5),
        ]);
        let x = try_apply_chunker(ten_chunker, stream, None, 0)
            .try_collect::<Vec<_>>()
            .await?;
        assert_eq!(
            x,
            vec![vec![1, 2, 3, 4], vec![5], vec![6, 3], vec![13], vec![4, 5],]
        );

        // A stream with values and errors yields groups which add up to ten, with interleaved errors
        let stream = stream::iter(vec![
            Ok::<_, anyhow::Error>(1),
            Ok(2),
            Err(anyhow::anyhow!("ERR")),
            Ok(3),
            Ok(4),
            Ok(5),
            Ok(6),
            Err(anyhow::anyhow!("ERR")),
            Ok(3),
            Ok(13),
            Ok(4),
            Ok(5),
        ]);
        let x = try_apply_chunker(ten_chunker, stream, None, 0)
            .into_stream()
            .collect::<Vec<_>>()
            .await;
        assert!(x[0].is_err());
        assert_eq!(*x[1].as_ref().unwrap(), vec![1, 2, 3, 4]);
        assert_eq!(*x[2].as_ref().unwrap(), vec![5]);
        assert!(x[3].is_err());
        assert_eq!(*x[4].as_ref().unwrap(), vec![6, 3]);
        assert_eq!(*x[5].as_ref().unwrap(), vec![13]);
        assert_eq!(*x[6].as_ref().unwrap(), vec![4, 5]);

        Ok(())
    }
}
