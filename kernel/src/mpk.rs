use x86_64::VirtAddr;

/// PKRS Register MSR Address
const MSR_PKRS: u32 = 0x6E1;

/// Enable PKS (Protection Keys for Supervisor Pages) in CR4
pub fn enable_pks() {
    unsafe {
        let mut cr4: u64;
        core::arch::asm!("mov {}, cr4", out(reg) cr4);
        
        // CR4 Bit 24 is PKS (Protection Keys for Supervisor Pages)
        cr4 |= 1 << 24; 
        
        core::arch::asm!("mov cr4, {}", in(reg) cr4);
        
        // Initialize PKRS: 
        // Key 0 is fully accessible (AD=0, WD=0).
        // Other keys can be set to AD=0/WD=0 by default or locked down.
        // We set everything to accessible initially.
        write_pkrs(0);
    }
}

/// Write to the PKRS MSR to configure access rights for supervisor keys
/// The layout is 2 bits per key (16 keys). 
/// Bit 0: AD (Access Disable), Bit 1: WD (Write Disable)
#[inline(always)]
pub fn write_pkrs(pkrs_value: u64) {
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") MSR_PKRS,
            in("eax") pkrs_value as u32,
            in("edx") (pkrs_value >> 32) as u32
        );
    }
}

/// Helper to lock down a specific key (AD=1, WD=1) or allow it (AD=0, WD=0)
/// Returns the new PKRS value to be written.
pub fn set_key_rights(current_pkrs: u64, key: u8, disable_access: bool, disable_write: bool) -> u64 {
    assert!(key < 16, "Key must be between 0 and 15");
    let shift = key * 2;
    let mut new_pkrs = current_pkrs & !(0b11 << shift); // clear bits
    
    if disable_access {
        new_pkrs |= 1 << shift;
    }
    if disable_write {
        new_pkrs |= 2 << shift;
    }
    
    new_pkrs
}

/// Manually walk the page table starting from CR3 to find the PTE for the given virtual address,
/// and tag it with the specified Protection Key (bits 59-62).
/// This completely bypasses the Rust type system limitations of the x86_64 crate.
pub fn tag_page(addr: VirtAddr, key: u8, hhdm_offset: u64) {
    assert!(key < 16, "Protection key must be between 0 and 15");
    
    unsafe {
        let mut cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        let pml4_phys = cr3 & 0x000F_FFFF_FFFF_F000;
        
        let p4_idx = addr.p4_index();
        let p3_idx = addr.p3_index();
        let p2_idx = addr.p2_index();
        let p1_idx = addr.p1_index();

        // Level 4
        let p4_ptr = (pml4_phys + hhdm_offset) as *mut u64;
        let p4_entry = p4_ptr.add(usize::from(p4_idx)).read_volatile();
        assert!(p4_entry & 1 != 0, "P4 entry not present");
        let pml3_phys = p4_entry & 0x000F_FFFF_FFFF_F000;

        // Level 3
        let p3_ptr = (pml3_phys + hhdm_offset) as *mut u64;
        let p3_entry = p3_ptr.add(usize::from(p3_idx)).read_volatile();
        assert!(p3_entry & 1 != 0, "P3 entry not present");
        assert!(p3_entry & (1 << 7) == 0, "1GB pages not supported for MPK tagging");
        let pml2_phys = p3_entry & 0x000F_FFFF_FFFF_F000;

        // Level 2
        let p2_ptr = (pml2_phys + hhdm_offset) as *mut u64;
        let p2_entry = p2_ptr.add(usize::from(p2_idx)).read_volatile();
        assert!(p2_entry & 1 != 0, "P2 entry not present");
        assert!(p2_entry & (1 << 7) == 0, "2MB pages not supported for MPK tagging");
        let pml1_phys = p2_entry & 0x000F_FFFF_FFFF_F000;

        // Level 1 (4KB Page)
        let p1_ptr = (pml1_phys + hhdm_offset) as *mut u64;
        let p1_entry_ptr = p1_ptr.add(usize::from(p1_idx));
        let mut p1_entry = p1_entry_ptr.read_volatile();
        assert!(p1_entry & 1 != 0, "P1 entry not present");

        // Clear existing Protection Key (bits 59-62)
        p1_entry &= !(0b1111u64 << 59);
        // Set new Protection Key
        p1_entry |= (key as u64) << 59;

        // Write the new PTE back
        p1_entry_ptr.write_volatile(p1_entry);
        
        // Invalidate the TLB for this page so the CPU picks up the new Protection Key
        core::arch::asm!("invlpg [{}]", in(reg) addr.as_u64());
    }
}
