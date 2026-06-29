use alloc::vec::Vec;
use alloc::alloc::{alloc, Layout};
use core::ptr;
use spin::Mutex;

const STACK_SIZE: usize = 16 * 1024; // 16 KB stack

pub struct Thread {
    pub id: u64,
    pub rsp: u64,
    pub is_active: bool,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    pub fn new(id: u64, entry: extern "C" fn()) -> Self {
        let layout = Layout::from_size_align(STACK_SIZE, 16).unwrap();
        let stack_ptr = unsafe { alloc(layout) };
        let stack_top = stack_ptr as u64 + STACK_SIZE as u64;

        let mut rsp = stack_top;

        unsafe {
            // Setup InterruptStackFrame structure pushed by hardware
            // Push SS (Stack Segment, 0 is valid in 64-bit Ring 0)
            rsp -= 8;
            ptr::write(rsp as *mut u64, 0);
            
            // Push RSP
            let _initial_rsp = rsp; // It's fine to point to itself minus what we pushed, or just the stack_top
            rsp -= 8;
            ptr::write(rsp as *mut u64, stack_top);
            
            // Push RFLAGS (Enable Interrupts)
            rsp -= 8;
            ptr::write(rsp as *mut u64, 0x202);
            
            // Push CS (Code Segment)
            rsp -= 8;
            ptr::write(rsp as *mut u64, 0x08);
            
            // Push RIP (Entry point)
            rsp -= 8;
            ptr::write(rsp as *mut u64, entry as u64);

            // Setup Context structure pushed by our naked handler
            // 15 general purpose registers
            for _ in 0..15 {
                rsp -= 8;
                ptr::write(rsp as *mut u64, 0);
            }
        }

        Thread {
            id,
            rsp,
            is_active: true,
        }
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler {
    threads: Vec::new(),
    current_idx: 0,
});

pub struct Scheduler {
    pub threads: Vec<Thread>,
    pub current_idx: usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            threads: Vec::new(),
            current_idx: 0,
        }
    }

    pub fn spawn(&mut self, thread: Thread) {
        self.threads.push(thread);
    }

    pub fn next_task_rsp(&mut self, current_rsp: u64) -> u64 {
        if self.threads.is_empty() {
            return current_rsp;
        }

        self.threads[self.current_idx].rsp = current_rsp;
        self.current_idx = (self.current_idx + 1) % self.threads.len();

        self.threads[self.current_idx].rsp
    }
}
