use limine::mp::MpInfo;

#[no_mangle]
pub unsafe extern "C" fn ap_main(info: &MpInfo) -> ! {
    let lapic_id = info.lapic_id;
    
    // Our serial_println! macro uses a spinlock, so it's safe to call concurrently.
    crate::serial_println!("[ AP {} ] Online and parked.", lapic_id);

    loop {
        unsafe { core::arch::asm!("hlt") }
    }
}
