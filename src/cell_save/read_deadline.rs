//! A stalled browser read must release the paging slot without inventing data.
use std::future::{Future, poll_fn};
use std::task::Poll;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(crate) const TIMED_OUT: &str = "saved terrain read timed out";

/// IndexedDB open requests cannot be aborted. Keep one permit until the
/// underlying request really finishes, even if its caller has timed out.
#[derive(Default)]
pub(crate) struct OpenGate(Rc<Cell<bool>>);
pub(crate) struct OpenPermit(Rc<Cell<bool>>);
impl OpenGate {
    pub(crate) fn start(&self) -> Option<OpenPermit> {
        if self.0.replace(true) {
            None
        } else {
            Some(OpenPermit(self.0.clone()))
        }
    }
}
impl Drop for OpenPermit {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

struct Delivery<T> {
    wanted: bool,
    value: Option<T>,
}
pub(crate) struct Receiver<T>(Rc<RefCell<Delivery<T>>>);
pub(crate) struct Sender<T>(Rc<RefCell<Delivery<T>>>);
pub(crate) fn delivery<T>() -> (Receiver<T>, Sender<T>) {
    let state = Rc::new(RefCell::new(Delivery {
        wanted: true,
        value: None,
    }));
    (Receiver(state.clone()), Sender(state))
}
impl<T> Receiver<T> {
    pub(crate) fn take(&self) -> Option<T> {
        self.0.borrow_mut().value.take()
    }
}
impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = self.0.borrow_mut();
        state.wanted = false;
        state.value.take();
    }
}
impl<T> Sender<T> {
    pub(crate) fn deliver(self, value: T) {
        let mut state = self.0.borrow_mut();
        if state.wanted {
            state.value = Some(value);
        }
        // Otherwise dropping the value closes a late database connection.
    }
}

/// Dropping the losing read also drops its IndexedDB handlers and database
/// guard. It cannot later publish stale pages into a newer attempt.
pub(crate) async fn read<T>(
    operation: impl Future<Output = Result<T, String>>,
    deadline: impl Future<Output = ()>,
) -> Result<T, String> {
    let mut operation = std::pin::pin!(operation);
    let mut deadline = std::pin::pin!(deadline);
    poll_fn(|cx| {
        if let Poll::Ready(result) = operation.as_mut().poll(cx) {
            return Poll::Ready(result);
        }
        if deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(TIMED_OUT.into()));
        }
        Poll::Pending
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        pin::Pin,
        rc::Rc,
        task::{Context, Waker},
    };

    struct PendingRead(Rc<Cell<bool>>);
    impl Future for PendingRead {
        type Output = Result<Vec<u8>, String>;
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    impl Drop for PendingRead {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    #[test]
    fn pending_read_times_out_releases_its_resources_and_allows_retry() {
        let dropped = Rc::new(Cell::new(false));
        let busy = Cell::new(true);
        let expired = Cell::new(false);
        let saved_terrain = vec![7, 3, 9];
        let mut cx = Context::from_waker(Waker::noop());
        let task = async {
            let result = read(
                PendingRead(dropped.clone()),
                poll_fn(|_| {
                    if expired.get() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                }),
            )
            .await;
            busy.set(false); // Same completion path as the browser paging slot.
            result
        };
        let mut task = std::pin::pin!(task);
        assert!(task.as_mut().poll(&mut cx).is_pending());
        assert!(busy.get() && !dropped.get());
        expired.set(true);
        assert_eq!(
            task.as_mut().poll(&mut cx),
            Poll::Ready(Err(TIMED_OUT.into()))
        );
        assert!(dropped.get());
        assert!(!busy.get());
        let mut retry = std::pin::pin!(read(
            std::future::ready(Ok(saved_terrain.clone())),
            std::future::pending(),
        ));
        assert_eq!(retry.as_mut().poll(&mut cx), Poll::Ready(Ok(saved_terrain)));
    }

    #[test]
    fn completed_read_wins_without_waiting_and_errors_are_not_missing_pages() {
        let mut cx = Context::from_waker(Waker::noop());
        let mut ready = std::pin::pin!(read(std::future::ready(Ok(42)), std::future::pending()));
        assert_eq!(ready.as_mut().poll(&mut cx), Poll::Ready(Ok(42)));
        let mut error = std::pin::pin!(read::<u8>(
            std::future::ready(Err("damaged page".into())),
            std::future::ready(()),
        ));
        assert_eq!(
            error.as_mut().poll(&mut cx),
            Poll::Ready(Err("damaged page".into()))
        );
    }

    #[test]
    fn cancelling_the_whole_batch_drops_the_pending_read() {
        let dropped = Rc::new(Cell::new(false));
        {
            let mut task =
                std::pin::pin!(read(PendingRead(dropped.clone()), std::future::pending()));
            assert!(
                task.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
        assert!(dropped.get());
    }

    #[test]
    fn timed_out_open_cannot_accumulate_requests_or_deliver_a_late_connection() {
        let gate = OpenGate::default();
        let permit = gate.start().unwrap();
        let (receiver, sender) = delivery();
        drop(receiver); // The read deadline expires while open is unresolved.
        for _ in 0..100 {
            assert!(gate.start().is_none());
        }
        let closed = Rc::new(Cell::new(false));
        sender.deliver(PendingRead(closed.clone()));
        assert!(
            closed.get(),
            "late connection must close, not reach a new attempt"
        );
        drop(permit); // Only actual open completion releases the slot.
        assert!(gate.start().is_some());
    }

    #[test]
    fn cancelling_after_open_completes_still_closes_its_undelivered_connection() {
        let closed = Rc::new(Cell::new(false));
        let (receiver, sender) = delivery();
        sender.deliver(PendingRead(closed.clone()));
        drop(receiver);
        assert!(closed.get());
        let (receiver, sender) = delivery();
        sender.deliver(42);
        assert_eq!(receiver.take(), Some(42));
    }
}
