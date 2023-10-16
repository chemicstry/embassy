use alloc::sync::Arc;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use atomic_polyfill::{AtomicU32, Ordering};
use embassy_time::Instant;
use futures_util::Future;

use super::run_queue::RunQueueItem;
use super::util::{SyncUnsafeCell, UninitCell};
use super::{timer_queue, wake_task, waker, TaskHeader, TaskRef, STATE_RUN_QUEUED, STATE_SPAWNED};
use crate::SpawnToken;

#[repr(C)]
pub struct AllocTaskStorage<F: Future + 'static> {
    raw: TaskHeader,
    future: UninitCell<F>,
}

impl<F: Future + 'static> AllocTaskStorage<F> {
    const RAW_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::waker_clone,
        Self::waker_wake,
        Self::waker_wake_by_ref,
        Self::waker_drop,
    );

    /// Try to spawn the task.
    ///
    /// The `future` closure constructs the future. It's only called if spawning is
    /// actually possible. It is a closure instead of a simple `future: F` param to ensure
    /// the future is constructed in-place, avoiding a temporary copy in the stack thanks to
    /// NRVO optimizations.
    ///
    /// This function will fail if the task is already spawned and has not finished running.
    /// In this case, the error is delayed: a "poisoned" SpawnToken is returned, which will
    /// cause [`Spawner::spawn()`](super::Spawner::spawn) to return the error.
    ///
    /// Once the task has finished running, you may spawn it again. It is allowed to spawn it
    /// on a different executor.
    pub fn spawn(future: impl FnOnce() -> F) -> SpawnToken<impl Sized> {
        let header = TaskHeader {
            state: AtomicU32::new(STATE_SPAWNED | STATE_RUN_QUEUED),
            run_queue_item: RunQueueItem::new(),
            executor: SyncUnsafeCell::new(None),
            poll_fn: SyncUnsafeCell::new(Some(AllocTaskStorage::<F>::poll)),

            #[cfg(feature = "integrated-timers")]
            expires_at: SyncUnsafeCell::new(Instant::from_ticks(0)),
            #[cfg(feature = "integrated-timers")]
            timer_queue_item: timer_queue::TimerQueueItem::new(),
        };

        let storage = Arc::new(AllocTaskStorage {
            raw: header,
            future: UninitCell::uninit(),
        });

        unsafe {
            storage.future.write_in_place(future);
        }

        let storage_ptr = Arc::into_raw(storage);

        debug!("Arc: {}", storage_ptr);

        // NOTE(unsafe): #[repr(C)] allows us to cast between AllocTaskStorage and TaskHeader
        let task_ref = unsafe { TaskRef::from_ptr(storage_ptr as _) };

        return unsafe { SpawnToken::<F>::new(task_ref) };
    }

    unsafe fn waker(self: &Arc<Self>) -> Waker {
        let storage_ptr = Arc::into_raw(self.clone());

        Waker::from_raw(RawWaker::new(storage_ptr as _, &Self::RAW_WAKER_VTABLE))
    }

    unsafe fn poll(p: TaskRef) {
        // let this = &*(p.as_ptr() as *const AllocTaskStorage<F>);
        let this = Arc::from_raw(p.as_ptr() as *const AllocTaskStorage<F>);

        let future = Pin::new_unchecked(this.future.as_mut());
        let waker = this.waker();
        let mut cx = Context::from_waker(&waker);
        match future.poll(&mut cx) {
            Poll::Ready(_) => {
                this.future.drop_in_place();
                this.raw.state.fetch_and(!STATE_SPAWNED, Ordering::AcqRel);

                #[cfg(feature = "integrated-timers")]
                this.raw.expires_at.set(Instant::MAX);
            }
            Poll::Pending => {}
        }

        debug!("Ref count: {} {}", p.as_ptr(), Arc::strong_count(&this));
    }

    unsafe fn waker_clone(p: *const ()) -> RawWaker {
        debug!("waker_clone: {}", p);
        Arc::increment_strong_count(p as *const Self);
        RawWaker::new(p, &Self::RAW_WAKER_VTABLE)
    }

    unsafe fn waker_wake(p: *const ()) {
        debug!("waker_wake: {}", p);
        wake_task(TaskRef::from_ptr(p as *const TaskHeader))
    }

    unsafe fn waker_wake_by_ref(p: *const ()) {
        debug!("waker_wake_by_ref: {}", p);
        Arc::increment_strong_count(p as *const Self);
        wake_task(TaskRef::from_ptr(p as *const TaskHeader))
    }

    unsafe fn waker_drop(p: *const ()) {
        debug!("waker_drop: {}", p);
        Arc::decrement_strong_count(p as *const Self);
    }

    #[doc(hidden)]
    #[allow(dead_code)]
    fn _assert_sync(self) {
        fn assert_sync<T: Sync>(_: T) {}

        assert_sync(self)
    }
}
