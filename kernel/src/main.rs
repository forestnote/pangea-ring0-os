#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

// --- 既存のモジュール群 ---
mod writer;
mod gdt;
mod interrupts;
mod pmm;
mod memory;
mod allocator;
pub mod serial;
pub mod scheduler;

// --- 復元したハードウェア制御モジュール ---
pub mod apic;
pub mod task;

// --- 新規追加したSIP（ソフトウェア分離プロセス）モジュール ---
pub mod sip;

// --- ハードウェア分離・保護モジュール ---
pub mod cpu;

use core::panic::PanicInfo;
use limine::request::{FramebufferRequest, MemmapRequest, HhdmRequest};
use limine::BaseRevision;
use x86_64::VirtAddr;

use sip::{Sip, SipEnv};

// ★ IPC、ASH、フォーマットのインポート
use alloc::string::String;
use alloc::format;
use sip::ipc::{self, Sender, Receiver};
use sip::ash::{AshContext, CheriCap, Perms, Instruction, Reg, AshJit};

// ★ 仮想メモリマッパー(VMM)をグローバルに公開し、W^X防壁を操作可能にする
use spin::Mutex;
use x86_64::structures::paging::OffsetPageTable;
pub static CORE_VMMS: [Mutex<Option<OffsetPageTable<'static>>>; 256] = [const { Mutex::new(None) }; 256];
#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[link_section = ".requests"]
static MEMMAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[link_section = ".requests"]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[ KERNEL PANIC ] {}", info);
    serial_println!("\n[ KERNEL PANIC ] {}", info);
    loop { unsafe { core::arch::asm!("cli; hlt") } }
}

// ==========================================
// 極小 Ring 0 非同期エグゼキュータ (The Brain)
// ==========================================
pub mod executor {
    use alloc::boxed::Box;
    use core::future::Future;
    use core::pin::Pin;
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    use alloc::collections::VecDeque;

    pub struct Task { future: Pin<Box<dyn Future<Output = ()> + Send + 'static>> }

    impl Task {
        pub fn new(future: impl Future<Output = ()> + Send + 'static) -> Task {
            Task { future: Box::pin(future) }
        }
        pub fn poll(&mut self, context: &mut Context) -> Poll<()> {
            self.future.as_mut().poll(context)
        }
    }

    fn dummy_raw_waker() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker { dummy_raw_waker() }
        let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);
        RawWaker::new(core::ptr::null(), vtable)
    }

    pub fn dummy_waker() -> Waker {
        unsafe { Waker::from_raw(dummy_raw_waker()) }
    }

    pub struct SimpleExecutor { task_queue: VecDeque<Task> }

    impl SimpleExecutor {
        pub fn new() -> SimpleExecutor { SimpleExecutor { task_queue: VecDeque::new() } }
        pub fn spawn(&mut self, task: Task) { self.task_queue.push_back(task) }
        pub fn run(&mut self) {
            crate::println!("[ INFO ] Ring 0 Async Executor Started.");
            while let Some(mut task) = self.task_queue.pop_front() {
                let waker = dummy_waker();
                let mut context = Context::from_waker(&waker);
                match task.poll(&mut context) {
                    Poll::Ready(()) => {}
                    Poll::Pending => self.task_queue.push_back(task),
                }
            }
            crate::println!("[ INFO ] All tasks completed. System halting.");
        }
    }

    pub struct YieldNow { yielded: bool }
    impl Future for YieldNow {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.yielded { Poll::Ready(()) } else {
                self.yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
    pub async fn yield_now() { YieldNow { yielded: false }.await }
}

use executor::{SimpleExecutor, Task, yield_now};

// ==========================================
// ソフトウェア分離プロセス (SIP) エントリポイント
// ==========================================
async fn sip_alpha_main(env: SipEnv, sender: Sender<String>) {
    println!("\n[ SIP Alpha ] Online. ID: {:?}", env.id());
    serial_println!("[ SIP Alpha ] Online. ID: {:?}", env.id());

    for i in 1..=3 {
        println!("        -> [ SIP Alpha ] Yielding to simulate heavy computation...");
        yield_now().await;
        let message = format!("Highly Classified Data Core Segment #{}", i);
        println!("        -> [ SIP Alpha ] Sending data: '{}'", message);
        serial_println!("        -> [ SIP Alpha ] Sending data: '{}'", message);
        sender.send(message);
    }
    println!("[ SIP Alpha ] All payloads transmitted. Terminating.");
}

async fn sip_beta_main(env: SipEnv, receiver: Receiver<String>) {
    println!("[ SIP Beta ] Online. ID: {:?}. Awaiting encrypted payloads...", env.id());
    serial_println!("[ SIP Beta ] Online. ID: {:?}. Awaiting encrypted payloads...", env.id());

    for _ in 1..=3 {
        let received_data = receiver.recv().await;
        println!("        <- [ SIP Beta ] Intercepted: '{}'", received_data);
        serial_println!("        <- [ SIP Beta ] Intercepted: '{}'", received_data);
    }
    println!("[ SIP Beta ] All payloads received and secured. Terminating.");
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    assert!(BASE_REVISION.is_supported());

    unsafe { core::arch::asm!("cli") };

    serial_println!("=====================================");
    serial_println!("PangeaOS Ring 0: The Singularity Engine");
    serial_println!("=====================================");

    if let Some(framebuffer_response) = FRAMEBUFFER_REQUEST.response() {
        if let Some(framebuffer) = framebuffer_response.framebuffers().first() {
            let fb_ptr = framebuffer.address() as *mut u8;
            let pitch = framebuffer.pitch as usize;
            let width = framebuffer.width as usize;
            let height = framebuffer.height as usize;

            let bg_color: u32 = 0xFF0064FF;
            for y in 0..height {
                unsafe {
                    let row_ptr = fb_ptr.add(y * pitch) as *mut u32;
                    core::slice::from_raw_parts_mut(row_ptr, width).fill(bg_color);
                }
            }

            writer::init_writer(fb_ptr, width, height, pitch);

            // ★ バージョンとブートシグネチャを v0.0.2-1 に更新
            println!("PangeaOS v0.0.2-1: Hardware-Assisted Isolation (SMEP/SMAP/PKU).");

            gdt::init();
            interrupts::init_idt();

            // ★ Phase 5: ハードウェア支援の隔離・保護（SMEP/SMAP/PKU）を有効化
            cpu::init_features();
            println!("[+] Hardware Protection (SMEP/SMAP/PKU) Enabled.");
            println!("[ OK ] Ring 0 Exclusive GDT & True IDT Loaded.");

            interrupts::disable_pic();
            println!("[ OK ] 8259 PIC Disabled (All Masked).");

            if let (Some(mem_map_res), Some(hhdm_res)) = (MEMMAP_REQUEST.response(), HHDM_REQUEST.response()) {
                let mem_map = mem_map_res.entries();
                let hhdm_offset = hhdm_res.offset;

                pmm::PageFrameAllocator::init(mem_map, hhdm_offset);

                let usable_mb = pmm::PMM.lock().as_ref().unwrap().get_usable_ram_mb();
                println!("[ OK ] PMM Online. Usable RAM: {} MB", usable_mb);

                let phys_mem_offset = VirtAddr::new(hhdm_offset);
                let mut mapper = unsafe { memory::init_mapper(phys_mem_offset) };

                let mut allocator_guard = pmm::PMM.lock();
                let pmm_allocator = allocator_guard.as_mut().unwrap();
                allocator::init_heap(&mut mapper, pmm_allocator).expect("Heap initialization failed!");

                // Initialize APIC for BSP
                apic::init(hhdm_offset, &mut mapper, pmm_allocator);

                drop(allocator_guard);

                println!("[ OK ] Global Heap Mapped. Allocator Ready.");

                let bsp_lapic_id = apic::lapic_id();
                *CORE_VMMS[bsp_lapic_id as usize].lock() = Some(mapper);

            } else {
                panic!("Failed to get Memory Map or HHDM offset.");
            }

            // ==========================================
            // ★ Phase 4: ASH JIT & W^X Enforcer の実証 (v0.0.1-9: Ultimate Sandbox)
            // ==========================================
            println!("\n[ ASH ] Booting Ring 0 Sandbox VM (Ultimate Secure Mode)...");
            serial_println!("[ ASH ] Booting Ring 0 Sandbox VM (Ultimate Secure Mode)...");

            let mut data_buf = [0u8; 64];
            let mut state_buf = [0u64; 8];

            // Mock IPv4 Packet
            data_buf[0] = 0x45; // Version 4, IHL 5
            data_buf[12] = 192; // Src IP: 192.168.1.1
            data_buf[13] = 168;
            data_buf[14] = 1;
            data_buf[15] = 1;

            let mut ctx = unsafe {
                AshContext {
                    memory: CheriCap::new_root(data_buf.as_mut_ptr(), 64, Perms::RW),
                    state: CheriCap::new_root(state_buf.as_mut_ptr(), 8, Perms::RW),
                }
            };
            
            let bytecode = [
                // 1. Get initial TSC Time (Helper 0)
                Instruction::CallExt(0), // Sets R0 = TSC
                Instruction::StoreState(Reg::R0, 2), // Save initial time to state[2]
                
                // 2. Load Src IP (32-bit Endian-Aware)
                Instruction::LoadImm(Reg::R4, 12),
                Instruction::LoadNet32(Reg::R1, Reg::R4), // R1 = 0xC0A80101 (192.168.1.1)
                
                // 3. Print the IP address from within JIT! (Helper 1 takes R1)
                Instruction::CallExt(1),
                
                // 4. Do some work (Turing-Complete Loop)
                Instruction::LoadImm(Reg::R2, 0),
                Instruction::LoadImm(Reg::R3, 100),
                Instruction::Add(Reg::R2, Reg::R3),
                Instruction::LoopBwd(Reg::R3, 1),
                Instruction::StoreState(Reg::R2, 1), // Store loop sum
                
                // 5. Get final TSC Time
                Instruction::CallExt(0), // R0 = new TSC
                Instruction::LoadState(Reg::R3, 2), // R3 = initial TSC
                Instruction::Sub(Reg::R0, Reg::R3), // R0 = R0 - R3 (Elapsed TSC ticks)
                Instruction::StoreState(Reg::R0, 2), // Store elapsed time to state[2]

                // Accept packet (R0 = 1)
                Instruction::LoadImm(Reg::R0, 1),
                Instruction::Exit,
            ];

            println!("\n[ ASH JIT ] Compiling Bytecode to Native x86_64...");
            serial_println!("[ ASH JIT ] Compiling Bytecode to Native x86_64...");

            let mut jit = AshJit::new();
            if let Err(e) = jit.compile(&bytecode) {
                println!("[ ASH JIT FATAL ] Compilation Error: {}", e);
                serial_println!("[ ASH JIT FATAL ] Compilation Error: {}", e);
                loop { unsafe { core::arch::asm!("hlt") }; }
            }

            println!("[ ASH JIT ] Sealing Memory Page (W^X Enforcer Active)...");
            serial_println!("[ ASH JIT ] Sealing Memory Page (W^X Enforcer Active)...");
            jit.seal();

            println!("[ ASH JIT ] Emission Complete. Direct Execution Initiated...");
            serial_println!("[ ASH JIT ] Emission Complete. Direct Execution Initiated...");

            let native_result = unsafe { jit.execute(&mut ctx) };

            println!("        -> [ ASH JIT ] Native Result: {}", native_result);
            serial_println!("        -> [ ASH JIT ] Native Result: {}", native_result);
            println!("        -> [ ASH JIT ] Loop Calculation Sum (State[1]): {}", state_buf[1]);
            serial_println!("        -> [ ASH JIT ] Loop Calculation Sum (State[1]): {}", state_buf[1]);
            println!("        -> [ ASH JIT ] JIT Execution Time (TSC Ticks): {}", state_buf[2]);
            serial_println!("        -> [ ASH JIT ] JIT Execution Time (TSC Ticks): {}", state_buf[2]);

            // ==========================================
            // ★ Phase 4: SIPと非同期エグゼキュータの起動 (True Preemption)
            // ==========================================
            println!("\n[ TARGET ACQUIRED ] Igniting Fully Preemptive Zero-Cost Concurrency Engine...");

            let (tx, rx) = ipc::channel::<String>();
            *ALPHA_TX.lock() = Some(tx);
            *BETA_RX.lock() = Some(rx);

            let thread_alpha = scheduler::Thread::new(1, alpha_thread_entry);
            let thread_beta = scheduler::Thread::new(2, beta_thread_entry);
            let thread_idle = scheduler::Thread::new(0, idle_thread_entry);

            {
                let mut sched = scheduler::SCHEDULER.lock();
                sched.spawn(thread_idle);
                sched.spawn(thread_alpha);
                sched.spawn(thread_beta);
            }

            unsafe { core::arch::asm!("sti") };

            loop { unsafe { core::arch::asm!("hlt") }; }
        }
    }

    loop { unsafe { core::arch::asm!("cli; hlt") }; }
}

pub static ALPHA_TX: Mutex<Option<Sender<String>>> = Mutex::new(None);
pub static BETA_RX: Mutex<Option<Receiver<String>>> = Mutex::new(None);

pub extern "C" fn alpha_thread_entry() {
    let tx = ALPHA_TX.lock().take().unwrap();
    let mut executor = SimpleExecutor::new();
    executor.spawn(Task::new(Sip::spawn(|env| sip_alpha_main(env, tx))));
    executor.run();
    loop { unsafe { core::arch::asm!("hlt") } }
}

pub extern "C" fn beta_thread_entry() {
    let rx = BETA_RX.lock().take().unwrap();
    let mut executor = SimpleExecutor::new();
    executor.spawn(Task::new(Sip::spawn(|env| sip_beta_main(env, rx))));
    executor.run();
    loop { unsafe { core::arch::asm!("hlt") } }
}

pub extern "C" fn idle_thread_entry() {
    loop { unsafe { core::arch::asm!("hlt") } }
}
