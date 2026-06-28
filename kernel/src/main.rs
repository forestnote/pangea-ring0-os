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

// --- 復元したハードウェア制御モジュール ---
pub mod apic;
pub mod task;

// --- 新規追加したSIP（ソフトウェア分離プロセス）モジュール ---
pub mod sip;

use core::panic::PanicInfo;
use limine::request::{FramebufferRequest, MemmapRequest, HhdmRequest};
use limine::BaseRevision;
use x86_64::VirtAddr;

use sip::{Sip, SipEnv};

// ★ IPCモジュールと文字列フォーマットのインポート
use alloc::string::String;
use alloc::format;
use sip::ipc::{self, Sender, Receiver};

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

    pub struct Task {
        future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    }

    impl Task {
        pub fn new(future: impl Future<Output = ()> + Send + 'static) -> Task {
            Task {
                future: Box::pin(future),
            }
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

    pub struct SimpleExecutor {
        task_queue: VecDeque<Task>,
    }

    impl SimpleExecutor {
        pub fn new() -> SimpleExecutor {
            SimpleExecutor {
                task_queue: VecDeque::new(),
            }
        }
        pub fn spawn(&mut self, task: Task) {
            self.task_queue.push_back(task)
        }
        pub fn run(&mut self) {
            crate::println!("[ INFO ] Ring 0 Async Executor Started.");
            // タスクキューが空になるまで非同期タスクを回し続ける
            while let Some(mut task) = self.task_queue.pop_front() {
                let waker = dummy_waker();
                let mut context = Context::from_waker(&waker);
                match task.poll(&mut context) {
                    Poll::Ready(()) => {} // タスク完了
                    Poll::Pending => self.task_queue.push_back(task), // まだ終わっていない場合は末尾に戻す
                }
            }
            crate::println!("[ INFO ] All tasks completed. System halting.");
        }
    }

    // 非同期タスクを意図的に1サイクルだけ休止させるダミーFuture
    pub struct YieldNow { yielded: bool }
    impl Future for YieldNow {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.yielded {
                Poll::Ready(())
            } else {
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

// 【SIP Alpha: 送信側プロセス】
async fn sip_alpha_main(env: SipEnv, sender: Sender<String>) {
    println!("\n[ SIP Alpha ] Online. ID: {:?}", env.id());
    serial_println!("[ SIP Alpha ] Online. ID: {:?}", env.id());

    for i in 1..=3 {
        println!("        -> [ SIP Alpha ] Yielding to simulate heavy computation...");
        yield_now().await;

        // ヒープに動的メモリを確保（String）
        let message = format!("Highly Classified Data Core Segment #{}", i);
        println!("        -> [ SIP Alpha ] Sending data: '{}'", message);
        serial_println!("        -> [ SIP Alpha ] Sending data: '{}'", message);

        // データの所有権をIPCチャネルへ投下（ムーブ）。
        // これ以降、Alphaはこの message にアクセスできない（コンパイラが遮断）。
        sender.send(message);
    }
    println!("[ SIP Alpha ] All payloads transmitted. Terminating.");
    serial_println!("[ SIP Alpha ] All payloads transmitted. Terminating.");
}

// 【SIP Beta: 受信側プロセス】
async fn sip_beta_main(env: SipEnv, receiver: Receiver<String>) {
    println!("[ SIP Beta ] Online. ID: {:?}. Awaiting encrypted payloads...", env.id());
    serial_println!("[ SIP Beta ] Online. ID: {:?}. Awaiting encrypted payloads...", env.id());

    for _ in 1..=3 {
        // データが到着するまで非同期で待機（CPUを手放して休止する）
        let received_data = receiver.recv().await;

        // 受け取ったデータの所有権はBetaにある。メモリコピーは一切発生していない（ゼロコピー）。
        println!("        <- [ SIP Beta ] Intercepted: '{}'", received_data);
        serial_println!("        <- [ SIP Beta ] Intercepted: '{}'", received_data);
    }
    println!("[ SIP Beta ] All payloads received and secured. Terminating.");
    serial_println!("[ SIP Beta ] All payloads received and secured. Terminating.");
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
            // ★ ブートシグネチャを v0.0.1-5 に更新
            println!("PangeaOS v0.0.1-5: Software-Isolated Barrier & Zero-Copy IPC.");

            gdt::init();
            interrupts::init_idt();
            println!("[ OK ] Ring 0 Exclusive GDT & True IDT Loaded.");

            // 8259 PICの沈黙
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
                drop(allocator_guard);

                println!("[ OK ] Global Heap Mapped. Allocator Ready.");

            } else {
                panic!("Failed to get Memory Map or HHDM offset.");
            }

            // ==========================================
            // ★ Phase 4: SIPと非同期エグゼキュータの起動
            // ==========================================
            println!("\n[ TARGET ACQUIRED ] Igniting Zero-Cost Concurrency Engine...");

            // 割り込みを許可し、APICタイマーを稼働させる
            unsafe { core::arch::asm!("sti") };

            let mut executor = SimpleExecutor::new();

            // ゼロコピーIPCチャネルを錬成
            let (tx, rx) = ipc::channel::<String>();

            // 2つのSIPを起動し、それぞれにチャネルの片割れ（権限）を渡す
            executor.spawn(Task::new(Sip::spawn(|env| sip_alpha_main(env, tx))));
            executor.spawn(Task::new(Sip::spawn(|env| sip_beta_main(env, rx))));

            // 非同期ランタイムの起動。SIPが完了するまでループする。
            executor.run();

        }
    }

    // すべての処理が完了したら、安全にCPUを休止させる
    loop { unsafe { core::arch::asm!("cli; hlt") }; }
}
