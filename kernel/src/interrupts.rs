use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use lazy_static::lazy_static;
use crate::{serial_println, println, gdt};
use pic8259::ChainedPics;
use spin::Mutex;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> = Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // CPU Exceptions (絶対防壁の要)
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt.general_protection_fault.set_handler_fn(gpf_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);

        // ★ ハッカーの迎撃作戦: ダブルフォルト時に強制的にIST(専用スタック)へコンテキストを退避させる
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }

        unsafe {
            idt[InterruptIndex::Timer.as_usize()]
                .set_handler_addr(x86_64::VirtAddr::new(timer_interrupt_handler_naked as *const () as u64));
        }

        idt
    };
}

pub fn init_idt() {
    IDT.load();
    serial_println!("[ OK ] True IDT Loaded. Exception Barrier Active.");
}

pub fn disable_pic() {
    unsafe { PICS.lock().initialize() };
    
    // Mask all interrupts (0xFF) to disable 8259 PIC entirely
    let mut pics = PICS.lock();
    unsafe {
        pics.write_masks(0xFF, 0xFF);
    }
    serial_println!("[ OK ] 8259 PIC Disabled (All Masked).");
}

#[unsafe(naked)]
pub extern "C" fn timer_interrupt_handler_naked() {
    core::arch::naked_asm!(
        "push rax",
        "push rcx",
        "push rdx",
        "push rbx",
        "push rbp",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov rdi, rsp",
        "and rsp, 0xFFFFFFFFFFFFFFF0", // Align stack for C ABI
        "call preempt_schedule",

        "mov r12, rax", // Save new_rsp across the next call
        "call apic_eoi_c",

        "mov rsp, r12", // Stack swap!

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rbp",
        "pop rbx",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq",
    );
}

#[no_mangle]
pub extern "C" fn preempt_schedule(old_rsp: u64) -> u64 {
    crate::task::timer::tick();
    crate::scheduler::SCHEDULER.lock().next_task_rsp(old_rsp)
}

#[no_mangle]
pub extern "C" fn apic_eoi_c() {
    crate::apic::eoi();
}
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("\n[ WARNING ] EXCEPTION: BREAKPOINT");
    serial_println!("\n[ WARNING ] EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn gpf_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    println!("\n[ KERNEL PANIC ] GENERAL PROTECTION FAULT");
    serial_println!("\n[ KERNEL PANIC ] GENERAL PROTECTION FAULT\nError Code: {}\n{:#?}", error_code, stack_frame);
    loop { unsafe { core::arch::asm!("hlt") } }
}

extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame, error_code: PageFaultErrorCode) {
    use x86_64::registers::control::Cr2;
    println!("\n[ KERNEL PANIC ] PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    serial_println!("\n[ KERNEL PANIC ] PAGE FAULT");
    serial_println!("Accessed Address: {:?}", Cr2::read());
    serial_println!("Error Code: {:?}", error_code);
    serial_println!("{:#?}", stack_frame);
    loop { unsafe { core::arch::asm!("hlt") } }
}

extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame, _error_code: u64) -> ! {
    println!("\n[ KERNEL PANIC ] DOUBLE FAULT (IST STACK ENGAGED)");
    serial_println!("\n[ KERNEL PANIC ] DOUBLE FAULT (IST STACK ENGAGED)\n{:#?}", stack_frame);
    loop { unsafe { core::arch::asm!("hlt") } }
}
