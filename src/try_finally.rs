use futures::prelude::*;
use std::panic::{AssertUnwindSafe, UnwindSafe};

/// An approximation of a "try/finally" block for async Result-returning code.
///
/// This function can be used to simulate a "try/finally" block that may be
/// familiar from languages with an exception system. The provided `body` closure
/// will be called and awaited and its result returned. The provided `finally` closure
/// will be called and awaited at the completion of the body, regardless of whether
/// the body returned successfully, returned an error, or panicked.
///
/// This pattern can be useful for running cleanup code, for example in cleaning up
/// after running a test. Rust code would typically use a scope guard with a `Drop` impl
/// for doing such cleanup work, but that does handle code that needs to be async.
///
/// # Example
///
/// ```
/// use cobalt_async::{try_finally};
/// use anyhow::Ok;
///
/// # tokio_test::block_on(async {
/// let mut cleaned_up = false;
///
/// let res = try_finally(
///   || async {
///     Ok("the body ran successfully")
///   },
///   || async {
///     cleaned_up = true;
///     Ok(())
///   }
/// ).await.unwrap();
///
/// assert_eq!(res, "the body ran successfully");
/// assert!(cleaned_up);
/// # })
/// ```
///
/// If the body closure references state that is not `UnwindSafe`, you may need
/// to manually annotate it with `AssertUnwindSafe`, and ensure that the finally
/// closure does not manipulate that state in a way that would violate unwind safety.
///
/// ```
/// use cobalt_async::{try_finally};
/// use anyhow::Ok;
/// use std::cell::RefCell;
///
/// # tokio_test::block_on(async {
/// let mut cleaned_up = false;
/// let mut data = RefCell::new(0);
///
/// // If the body code panics, we promise not to look at `data` in the finally code.
/// let data = std::panic::AssertUnwindSafe(data);
///
/// let res = try_finally(
///   || async {
///     Ok(data.replace(42))
///   },
///   || async {
///     cleaned_up = true;
///     Ok(())
///   }
/// ).await.unwrap();
///
/// assert_eq!(res, 0);
/// assert_eq!(data.take(), 42);
/// assert!(cleaned_up);
/// # })
/// ```
///
/// # Notes
///
/// * As with `Drop`, running the `finally` closure on panic is not guaranteed.
///   In particular it will not be run when the panic mode is set to abort.
///
/// * Panics in the `finally` code will not be caught, and may potentially mask
///   panics from the `body` code.
///
/// * Errors returned by the `finally` code will be returned to the caller, potentially
///   masking errors returned by the `body` code.
///
pub async fn try_finally<B, F, BFut, FFut, Out, Err>(body: B, finally: F) -> Result<Out, Err>
where
    B: FnOnce() -> BFut,
    B: UnwindSafe,
    BFut: Future<Output = Result<Out, Err>>,
    F: FnOnce() -> FFut,
    FFut: Future<Output = Result<(), Err>>,
{
    // Here we want to call the "body closure" to produce a "body future",
    // attempt to run the body-future to completion. Regardless of whether
    // that succeeds or fails, we want to call a "finally closure" to produce a
    // "finally future" that we run to completion before returning the result
    // of the body-future to the caller.
    //
    // Importantly, we want to run the finally-future even if the body-future panics,
    // so that this helper can be conveniently used for writing tests. We therefore need
    // to be careful about "exception safety" as described in [1].
    //
    // This is all safe code, so the key thing we need to ensure here is that
    // we do not allow the code to observe any state invariants that may have been
    // left in a broken state by the panic. We reason as follows:
    //
    //  * The body-closure is constrained to be `UnwindSafe`, so any outside state
    //    that it depends on will not have invariants that need to be maintained
    //    across panics.
    //
    //  * The body-future is not necessarily `UnwindSafe`, since it may contain any
    //    arbitrary data that's owned by the future. To be able to deal with panics
    //    resulting from polling the body-future, we must wrap it in an `AssertUnwindSafe`,
    //    making us responsible for preventing anyone from observing it in an invalid state.
    //
    //  * If the body-future does panic, we will execute the finally-future and then
    //    re-trigger the panic. So the only thing that could observe broken invariants
    //    in the state of the body-future, is the finally-future.
    //
    //  * The finally-future can't see any data that's referenced by the body-future, unless
    //    that data was closed over by both the body-closure and the finally-closure.
    //
    //  * But we know that anything closed over by the body-closure is `UnwindSafe`.
    //
    //  * So, we know that we can run the finally-future to completion without it observing any
    //    broken invariants from the panic, and our `AssertUnwindSafe` is sound.
    //
    // [1] https://github.com/rust-lang/rfcs/blob/master/text/1236-stabilize-catch-panic.md

    // So, first we run the body code and catch any unwinding panics.
    // (This won't help if doing an aborting panic, but then there's *nothing* we can do).
    let body = AssertUnwindSafe(body());
    let body_res = body.catch_unwind().await;

    // Run the finally code.
    // Note that this may itself panic, which we do not catch.
    let finally_res = finally().await;

    // The final result depends on both body behaviour, and finally behaviour.
    match (body_res, finally_res) {
        // If the body call panicked, trigger it again after cleanup.
        (Err(cause), _) => std::panic::resume_unwind(cause),

        // If the cleanup failed, that error takes precedence over the body result.
        (_, Err(finally_err)) => Err(finally_err),

        // Otherwise, the result from the body is returned.
        (Ok(res), Ok(())) => res,
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use anyhow::{bail, Ok};

    #[tokio::test]
    async fn test_finally_runs_after_success() {
        let mut cleaned_up = false;

        let res = try_finally(
            || async { Ok("ok!") },
            || async {
                cleaned_up = true;
                Ok(())
            },
        )
        .await
        .unwrap();

        assert_eq!(res, "ok!");
        assert!(cleaned_up);
    }

    #[tokio::test]
    async fn test_finally_runs_after_failure() {
        let mut cleaned_up = false;

        let err = try_finally(
            || async {
                bail!("oh no!");
                // The below is for type inference.
                #[allow(unreachable_code)]
                Ok(())
            },
            || async {
                cleaned_up = true;
                Ok(())
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.to_string(), "oh no!");
        assert!(cleaned_up);
    }

    #[tokio::test]
    #[should_panic(expected = "in the cleanup!")]
    async fn test_finally_runs_after_panic() {
        try_finally(
            || async {
                panic!("at the disco!");
                // The below is for type inference.
                #[allow(unreachable_code)]
                Ok(())
            },
            || async {
                // Panic again in the cleanup code, with a different message.
                // This lets us use `#[should_panic]` rather than trying to catch
                // the panic *again* in the test and assert that cleanup code was run.
                panic!("in the cleanup!")
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_finally_errors_are_propagated() {
        let err = try_finally(
            || async { Ok("sucess!") },
            || async { bail!("cleanup failed!") },
        )
        .await
        .unwrap_err();

        assert_eq!(err.to_string(), "cleanup failed!");
    }

    #[tokio::test]
    async fn test_finally_errors_mask_body_errors() {
        // This behaviour is a little unfortunate, but it's not obvious how to
        // implement something better in a generic way.
        let err = try_finally(
            || async {
                bail!("body failed!");
                // The below is for type inference.
                #[allow(unreachable_code)]
                Ok(())
            },
            || async { bail!("cleanup failed!") },
        )
        .await
        .unwrap_err();

        assert_eq!(err.to_string(), "cleanup failed!");
    }
}
