use x86_64::registers::model_specific::Msr;

const APIC_SIVR: u64 = 0xF0;
const APIC_EOI: u64 = 0xB0;
const APIC_LVT_TIMER: u64 = 0x320;
const APIC_TIMER_INIT: u64 = 0x380;
// const APIC_TIMER_CURR: u64 = 0x390;
const APIC_TIMER_DIV: u64 = 0x3E0;
const APIC_ID: u64 = 0x20;

use x86_64::structures::paging::{Page, PhysFrame, Mapper, Size4KiB, PageTableFlags, FrameAllocator};
use x86_64::{VirtAddr, PhysAddr};

static mut APIC_BASE_VIRT: u64 = 0;

pub fn init(
    hhdm_offset: u64,
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    let msr = Msr::new(0x1B); // IA32_APIC_BASE
    let phys_base = unsafe { msr.read() } & 0xFFFF_FFFF_FFFF_F000;
    let virt_base = phys_base + hhdm_offset;
    
    // Ensure the APIC MMIO region is mapped and writable
    let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_base));
    let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(phys_base));
    // Cache disable (NO_CACHE / NO_CACHE_LEVEL_2 equivalent) is important for MMIO
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;
    
    unsafe {
        // It might already be mapped by Limine, so we use `update_flags` if map_to fails,
        // or just try to map it and ignore the error if it already exists.
        // x86_64 crate's map_to returns an error if already mapped.
        match mapper.map_to(page, frame, flags, frame_allocator) {
            Ok(tlb) => tlb.flush(),
            Err(x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)) => {
                // If it's already mapped, let's update flags to ensure it's writable and NO_CACHE
                if let Ok(tlb) = mapper.update_flags(page, flags) {
                    tlb.flush();
                }
            }
            Err(e) => panic!("Failed to map APIC base: {:?}", e),
        }
    }

    unsafe { APIC_BASE_VIRT = virt_base; }

    crate::serial_println!("[ INFO ] APIC Physical Base: 0x{:X}", phys_base);
    crate::serial_println!("[ INFO ] APIC Virtual Base: 0x{:X}", unsafe { APIC_BASE_VIRT });

    // Enable APIC by writing to the Spurious Interrupt Vector Register (SIVR)
    // Bit 8 enables the APIC. We map spurious interrupts to vector 0xFF.
    write_reg(APIC_SIVR, 0xFF | 0x100);

    // Configure Timer
    // Vector 32 (0x20 is PIC_1_OFFSET, matching our IDT Timer handler).
    // Mode: Periodic (Bit 17) -> 0x20000
    // Total: 0x20020
    write_reg(APIC_LVT_TIMER, 32 | 0x20000);
    
    // Divide Configuration
    // 0x3 means divide by 16
    write_reg(APIC_TIMER_DIV, 0x3);
    
    // Initial Count
    // In a real OS we'd calibrate this. 10_000_000 is usually around 10-100Hz on modern CPUs.
    write_reg(APIC_TIMER_INIT, 10_000_000);
    
    crate::serial_println!("[ OK ] Local APIC Initialized. Timer Active.");
}

pub fn init_ap() {
    // Enable APIC for this core
    write_reg(APIC_SIVR, 0xFF | 0x100);

    // Configure Timer
    write_reg(APIC_LVT_TIMER, 32 | 0x20000);
    write_reg(APIC_TIMER_DIV, 0x3);
    write_reg(APIC_TIMER_INIT, 10_000_000);
}

pub fn eoi() {
    write_reg(APIC_EOI, 0);
}

fn write_reg(offset: u64, value: u32) {
    unsafe {
        if APIC_BASE_VIRT == 0 {
            panic!("APIC write before init!");
        }
        let ptr = (APIC_BASE_VIRT + offset) as *mut u32;
        core::ptr::write_volatile(ptr, value);
    }
}

pub fn read_reg(offset: u64) -> u32 {
    unsafe {
        if APIC_BASE_VIRT == 0 {
            panic!("APIC read before init!");
        }
        let ptr = (APIC_BASE_VIRT + offset) as *const u32;
        core::ptr::read_volatile(ptr)
    }
}

pub fn lapic_id() -> u8 {
    (read_reg(APIC_ID) >> 24) as u8
}
