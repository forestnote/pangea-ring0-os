#![no_std]
#![no_main]

mod writer;
pub mod serial;

use core::panic::PanicInfo;
use limine::request::FramebufferRequest;
use limine::BaseRevision;

#[used]
#[link_section = ".requests"]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[link_section = ".requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[ KERNEL PANIC ] {}", info);
    serial_println!("\n[ KERNEL PANIC ] {}", info);
    loop { unsafe { core::arch::asm!("pause") } }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    assert!(BASE_REVISION.is_supported());

    // 全てのハードウェア割り込みを物理的に遮断
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

            // 初期画面の高速クリア
            let bg_color: u32 = 0xFF0064FF;
            for y in 0..height {
                unsafe {
                    let row_ptr = fb_ptr.add(y * pitch) as *mut u32;
                    let row_slice = core::slice::from_raw_parts_mut(row_ptr, width);
                    row_slice.fill(bg_color);
                }
            }

            writer::init_writer(fb_ptr, width, height, pitch);
            // ★ バージョンを v0.0.1-2 へ更新
            println!("PangeaOS v0.0.1-2: Zero-Interrupt Polling Engine.");
            println!("[ OK ] Hardware Interrupts completely ANNIHILATED.");
            println!("[ OK ] Using 100% CPU Direct Buffer Draining bypass.");

            println!("\n[ INFO ] Initiating Buffer Test...");
            for i in 1..=60 {
                println!("Memory Block Allocator Trace -> Address: 0x{:08X}", i * 0x1000);
            }

            println!("\n[ TARGET ACQUIRED ]");
            println!("System is locked into a pure polling state.");
            println!("Click this QEMU window and press [PageUp] or [ArrowUp].");

            use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1, KeyCode};
            let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);

            // 無限監視ループ
            loop {
                unsafe {
                    let mut port_64 = x86_64::instructions::port::Port::<u8>::new(0x64);
                    let mut port_60 = x86_64::instructions::port::Port::<u8>::new(0x60);

                    // ★ 修正: if ではなく while を使用し、ハードウェアバッファに
                    // 溜まったデータを「1滴残らず完全に吸い出す」まで処理を止めない
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
                    // CPUの熱暴走を防ぎつつ超高速でループを回す
                    core::arch::asm!("pause");
                }
            }
        }
    }

    loop { unsafe { core::arch::asm!("pause") }; }
}
