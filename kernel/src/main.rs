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
            println!("PangeaOS v0.0.1-2: Memory Defense Engaged.");

            gdt::init();
            interrupts::init_idt();
            println!("[ OK ] Zero-Interrupt Polling Engine Online.");

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

                allocator::init_heap(&mut mapper, pmm_allocator).expect("Heap initialization failed!");

                drop(allocator_guard);

                println!("[ OK ] Heap Space Mapped. Engaging Rust 'alloc' Ecosystem...");

                // 動的メモリ確保の実証実験 (Box, Vec, String)
                let heap_value = Box::new(0x1337_C0DE_DEAD_BEEF_u64);
                println!("       -> Boxed value at {:p} = 0x{:016X}", heap_value, *heap_value);

                let mut vec = Vec::new();
                for i in 0..500 {
                    vec.push(i);
                }
                println!("       -> Vec allocated dynamically. Length: {}, Capacity: {}", vec.len(), vec.capacity());

                let string = String::from("PangeaOS Dynamic Memory Routing Online.");
                println!("       -> String allocated: {}", string);
                serial_println!("[ SUCCESS ] Allocator verified.");

            } else {
                panic!("Failed to get Memory Map or HHDM offset from Limine.");
            }

            println!("\n[ TARGET ACQUIRED ]");
            println!("System is locked into a pure polling state.");
            println!("Click this QEMU window and press [PageUp] or [ArrowUp].");

            use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1, KeyCode};
            let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);

            loop {
                unsafe {
                    let mut port_64 = x86_64::instructions::port::Port::<u8>::new(0x64);
                    let mut port_60 = x86_64::instructions::port::Port::<u8>::new(0x60);

                    while (port_64.read() & 1) == 1 {
                        let scancode = port_60.read();

                        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                            if let Some(key) = keyboard.process_keyevent(key_event) {
                                match key {
                                    DecodedKey::RawKey(raw) => {
                                        match raw {
                                            KeyCode::PageUp | KeyCode::ArrowUp => crate::writer::scroll_up(),
                                            KeyCode::PageDown | KeyCode::ArrowDown => crate::writer::scroll_down(),
                                            _ => {}
                                        }
                                    }
                                    DecodedKey::Unicode(character) => {
                                        crate::print!("{}", character);
                                    }
                                }
                            }
                        }
                    }
                    core::arch::asm!("pause");
                }
            }
        }
    }

    loop { unsafe { core::arch::asm!("pause") }; }
}
