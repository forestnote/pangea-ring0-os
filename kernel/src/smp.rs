use limine::mp::MpInfo;

#[no_mangle]
pub unsafe extern "C" fn ap_main(info: &MpInfo) -> ! {
    let lapic_id = info.lapic_id;
    
    crate::serial_println!("[ AP {} ] Online and active.", lapic_id);

    // Initialize GDT and IDT for this AP
    crate::gdt::init_ap();
    crate::interrupts::init_idt();

    // Initialize Local APIC for this core (already mapped by BSP)
    crate::apic::init_ap();

    // Enable interrupts for this core
    unsafe { core::arch::asm!("sti") };

    if lapic_id == 1 {
        crate::serial_println!("[ AP 1 ] Igniting ALPHA Thread...");
        crate::alpha_thread_entry();
    } else if lapic_id == 2 {
        crate::serial_println!("[ AP 2 ] Igniting BETA Thread...");
        crate::beta_thread_entry();
    }

    loop {
        unsafe { core::arch::asm!("hlt") }
    }
}
