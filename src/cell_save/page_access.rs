//! Coordinate final save publication with application of in-flight page replies.
//! Compression is outside this gate; neither operation changes validation rules.
use std::{
    cell::RefCell,
    future::poll_fn,
    rc::Rc,
    task::{Poll, Waker},
};
#[derive(Default)]
struct State {
    readers: usize,
    publishing: bool,
    waiter: Option<Waker>,
}
#[derive(Clone, Default)]
pub(crate) struct Gate(Rc<RefCell<State>>);
pub(crate) struct Read(Rc<RefCell<State>>);
pub(crate) struct Publication(Rc<RefCell<State>>);
impl Gate {
    pub(crate) fn read(&self) -> Option<Read> {
        let mut state = self.0.borrow_mut();
        if state.publishing {
            return None;
        }
        state.readers += 1;
        Some(Read(self.0.clone()))
    }
    pub(crate) async fn publish(&self) -> Option<Publication> {
        {
            let mut state = self.0.borrow_mut();
            if state.publishing {
                return None;
            }
            state.publishing = true;
        }
        let publication = Publication(self.0.clone());
        poll_fn(|cx| {
            let mut state = self.0.borrow_mut();
            if state.readers == 0 {
                Poll::Ready(())
            } else {
                state.waiter = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await;
        Some(publication)
    }
}
impl Drop for Read {
    fn drop(&mut self) {
        let wake = {
            let mut state = self.0.borrow_mut();
            state.readers -= 1;
            if state.readers == 0 {
                state.waiter.take()
            } else {
                None
            }
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        let mut state = self.0.borrow_mut();
        state.publishing = false;
        state.waiter = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{future::Future, task::Context};
    #[test]
    fn reply_must_be_applied_before_publication_and_reads_resume_after_commit() {
        let gate = Gate::default();
        let reply = gate.read().unwrap();
        let mut publication = Box::pin(gate.publish());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(publication.as_mut().poll(&mut cx).is_pending());
        assert!(gate.read().is_none());
        // Holding the reply after I/O completion still blocks publication.
        assert!(publication.as_mut().poll(&mut cx).is_pending());
        drop(reply);
        let Poll::Ready(Some(commit)) = publication.as_mut().poll(&mut cx) else {
            panic!("reply applied")
        };
        assert!(gate.read().is_none());
        drop(commit);
        assert!(gate.read().is_some());
    }
    #[test]
    fn canceled_publication_unblocks_reads_without_waiting_for_the_old_read() {
        let gate = Gate::default();
        let _read = gate.read().unwrap();
        let mut pending = Box::pin(gate.publish());
        assert!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        drop(pending);
        assert!(gate.read().is_some());
    }
    #[test]
    fn compression_does_not_hold_a_publication_gate_and_all_replies_must_drain() {
        let gate = Gate::default();
        let first = gate.read().unwrap();
        let last = gate.read().unwrap();
        let mut pending = Box::pin(gate.publish());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(pending.as_mut().poll(&mut cx).is_pending());
        drop(first);
        assert!(pending.as_mut().poll(&mut cx).is_pending());
        drop(last);
        assert!(pending.as_mut().poll(&mut cx).is_ready());
        assert!(gate.read().is_some());
    }
    #[test]
    fn applying_the_last_reply_wakes_the_waiting_publisher() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use std::task::Wake;
        struct Count(AtomicUsize);
        impl Wake for Count {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(count.clone());
        let gate = Gate::default();
        let reply = gate.read().unwrap();
        let mut publication = Box::pin(gate.publish());
        assert!(
            publication
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        drop(reply);
        assert_eq!(count.0.load(Ordering::Relaxed), 1);
        assert!(
            publication
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_ready()
        );
    }
    #[test]
    fn a_second_publisher_cannot_replace_the_waiter() {
        let gate = Gate::default();
        let _read = gate.read().unwrap();
        let mut first = Box::pin(gate.publish());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(first.as_mut().poll(&mut cx).is_pending());
        let mut second = Box::pin(gate.publish());
        assert!(matches!(second.as_mut().poll(&mut cx), Poll::Ready(None)));
        assert!(gate.read().is_none());
    }
}
