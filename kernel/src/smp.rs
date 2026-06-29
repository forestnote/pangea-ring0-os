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

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub static SHOOTDOWN_ADDR: AtomicU64 = AtomicU64::new(0);
pub static SHOOTDOWN_ACK: AtomicUsize = AtomicUsize::new(0);
pub static ACTIVE_CORES: AtomicUsize = AtomicUsize::new(1); // Updated during boot

pub fn tlb_shootdown(addr: u64) {
    let cores = ACTIVE_CORES.load(Ordering::Acquire);
    if cores <= 1 {
        // シングルコア環境の場合は自コアのTLBフラッシュのみで完了
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(addr));
        return;
    }

    SHOOTDOWN_ADDR.store(addr, Ordering::Release);
    SHOOTDOWN_ACK.store(0, Ordering::Release);
    
    // 他の全コア(自コアを除く)に対してTLB Shootdown割り込み(0x40)を送信
    crate::apic::send_ipi(0x40, 3);

    // 他の全コアからの応答(ACK)をスピンして待機 (厳密な同期)
    while SHOOTDOWN_ACK.load(Ordering::Acquire) < cores - 1 {
        core::hint::spin_loop();
    }
    
    // 自コアのTLBも最後にフラッシュ
    x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(addr));
}
