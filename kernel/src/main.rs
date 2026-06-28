#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod writer;
mod gdt;
mod interrupts;
mod pmm;
mod memory;
mod allocator;
pub mod serial;
pub mod task;
pub mod apic;

use core::panic::PanicInfo;
use limine::request::{FramebufferRequest, MemmapRequest, HhdmRequest};
use limine::BaseRevision;
use x86_64::VirtAddr;

use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::string::String;

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
    loop { unsafe { core::arch::asm!("pause") } }
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
            println!("PangeaOS v0.0.1-3: Async Singularity Awakened.");

            gdt::init();
            interrupts::init_idt();
            println!("[ OK ] GDT and IDT Loaded.");

            if let (Some(mem_map_res), Some(hhdm_res)) = (MEMMAP_REQUEST.response(), HHDM_REQUEST.response()) {
                let mem_map = mem_map_res.entries();
                let hhdm_offset = hhdm_res.offset;

                pmm::PageFrameAllocator::init(mem_map, hhdm_offset);
                let usable_mb = pmm::PMM.lock().as_ref().unwrap().get_usable_ram_mb();
                println!("[ OK ] Physical Memory Manager Online. Usable RAM: {} MB", usable_mb);

                // MMUの掌握
                let phys_mem_offset = VirtAddr::new(hhdm_offset);
                let mut mapper = unsafe { memory::init_mapper(phys_mem_offset) };

                // ==========================================
                // ★ Phase 3-1: Global Heap Allocator の起動と動的メモリテスト
                // ==========================================
                println!("\n[ INFO ] Initializing Ring 0 Global Allocator...");

                let mut allocator_guard = pmm::PMM.lock();
                let pmm_allocator = allocator_guard.as_mut().unwrap();

                // ==========================================
                // ★ Phase 3-3: Local APIC Initialization
                // ==========================================
                println!("\n[ INFO ] Engaging Local APIC (Modern Interrupts)...");
                interrupts::disable_pic();
                apic::init(hhdm_offset, &mut mapper, pmm_allocator);
                println!("[ OK ] Hybrid Async-Interrupt Engine Online (APIC Driven).");

                allocator::init_heap(&mut mapper, pmm_allocator).expect("Heap initialization failed!");

                drop(allocator_guard);

                println!("[ OK ] Heap Space Mapped. Engaging Rust 'alloc' Ecosystem...");

                // Enable interrupts ONLY AFTER the allocator is ready
                x86_64::instructions::interrupts::enable();
                crate::serial_println!("[ TRACE ] Interrupts Enabled.");

                // ==========================================
                // ★ Phase 3-2: True Mesh Allocator (Size-Class) 実証実験
                // ==========================================
                println!("\n[ INFO ] Testing True Mesh Allocator (Size-Class Reuse)...");
                let ptr1 = Box::into_raw(Box::new(0x1111_2222_3333_4444_u64));
                let ptr2 = Box::into_raw(Box::new(0x5555_6666_7777_8888_u64));
                println!("       -> Allocated Ptr1 at {:p}", ptr1);
                println!("       -> Allocated Ptr2 at {:p}", ptr2);

                unsafe {
                    drop(Box::from_raw(ptr1));
                    println!("       -> Dropped Ptr1.");
                }

                let ptr3 = Box::into_raw(Box::new(0x9999_AAAA_BBBB_CCCC_u64));
                println!("       -> Allocated Ptr3 at {:p} (Should match Ptr1 if reused!)", ptr3);
                
                if ptr1 == ptr3 {
                    println!("       -> [ OK ] Memory Successfully Reused!");
                    serial_println!("[ SUCCESS ] True Mesh Allocator verified. Memory reused.");
                } else {
                    println!("       -> [ FAIL ] Memory Not Reused.");
                    serial_println!("[ FAIL ] True Mesh Allocator failed to reuse memory.");
                }
                
                // Cleanup
                unsafe {
                    drop(Box::from_raw(ptr2));
                    drop(Box::from_raw(ptr3));
                }

                let mut vec = Vec::new();
                for i in 0..500 { vec.push(i); }
                let string = String::from("PangeaOS Mesh Allocator Online.");
                println!("       -> {} (Vec len: {})", string, vec.len());

            } else {
                panic!("Failed to get Memory Map or HHDM offset from Limine.");
            }

            println!("\n[ TARGET ACQUIRED ]");
            println!("System is locked into an Async-Await Task Executor.");
            println!("Click this QEMU window and press [PageUp] or [ArrowUp].");

            let mut executor = task::executor::Executor::new();
            
            // Spawn Keyboard Async Poller
            executor.spawn(task::Task::new(task::keyboard::keyboard_task()));
            
            // Spawn Timer Task
            executor.spawn(task::Task::new(async {
                let mut counter = 0;
                loop {
                    task::timer::sleep(5).await; // wait for 5 ticks
                    counter += 1;
                    crate::println!("[ TIMER ] Async Task Awake: {} cycles", counter);
                    crate::serial_println!("[ TIMER ] Async Task Awake: {} cycles", counter);
                }
            }));

            executor.run();
        }
    }

    loop { unsafe { core::arch::asm!("pause") }; }
}
