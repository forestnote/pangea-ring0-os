pub mod executor;
pub mod keyboard;
pub mod timer;

use core::{future::Future, pin::Pin};
use alloc::boxed::Box;
use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(usize);

impl TaskId {
    pub(crate) fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct Task {
    id: TaskId,
    future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Task {
        Task {
            id: TaskId::new(),
            future: Box::pin(future),
        }
    }

    fn poll(&mut self, context: &mut core::task::Context) -> core::task::Poll<()> {
        self.future.as_mut().poll(context)
    }
}
