use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use lazy_static::lazy_static;
use crate::{serial_println, println, gdt};

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

        idt
    };
}

pub fn init_idt() {
    IDT.load();
    serial_println!("[ OK ] True IDT Loaded. Exception Barrier Active.");
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
