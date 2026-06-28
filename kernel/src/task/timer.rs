use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use core::sync::atomic::{AtomicUsize, Ordering};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;
use x86_64::instructions::interrupts;

static TICKS: AtomicUsize = AtomicUsize::new(0);
static WAKERS: Mutex<BTreeMap<usize, Vec<Waker>>> = Mutex::new(BTreeMap::new());

pub fn get_ticks() -> usize {
    TICKS.load(Ordering::Relaxed)
}

pub fn tick() {
    let current = TICKS.fetch_add(1, Ordering::Relaxed);
    let target = current + 1;
    
    let mut to_wake = Vec::new();
    
    interrupts::without_interrupts(|| {
        let mut wakers = WAKERS.lock();
        let mut keys_to_remove = Vec::new();
        for (tick, waker_list) in wakers.iter() {
            if *tick <= target {
                keys_to_remove.push(*tick);
                for w in waker_list {
                    to_wake.push(w.clone());
                }
            } else {
                break;
            }
        }
        
        for key in keys_to_remove {
            wakers.remove(&key);
        }
    });
    
    for waker in to_wake {
        waker.wake();
    }
}

pub struct SleepFuture {
    target_tick: usize,
}

impl Future for SleepFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let current = TICKS.load(Ordering::Relaxed);
        if current >= self.target_tick {
            Poll::Ready(())
        } else {
            interrupts::without_interrupts(|| {
                let mut wakers = WAKERS.lock();
                let waker_list = wakers.entry(self.target_tick).or_insert_with(Vec::new);
                waker_list.push(cx.waker().clone());
            });
            Poll::Pending
        }
    }
}

pub fn sleep(ticks: usize) -> SleepFuture {
    let current = TICKS.load(Ordering::Relaxed);
    SleepFuture {
        target_tick: current + ticks,
    }
}
