use x86_64::VirtAddr;
use x86_64::structures::paging::{PageTable, OffsetPageTable, Page, PhysFrame, Mapper, Size4KiB, FrameAllocator};
use x86_64::registers::control::Cr3;

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

pub unsafe fn init_mapper(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

// 警告対策: 将来のMesh圧縮機能のために維持
#[allow(dead_code)]
pub fn create_mesh_mapping(
    page: Page<Size4KiB>,
    frame: PhysFrame<Size4KiB>,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    let flags = Flags::PRESENT | Flags::WRITABLE;

    unsafe {
        match mapper.map_to(page, frame, flags, frame_allocator) {
            Ok(tlb) => tlb.flush(),
            Err(e) => crate::println!("[ ERROR ] Failed to map memory: {:?}", e),
        }
    }
}

#[allow(dead_code)]
pub unsafe fn create_per_core_page_table(
    physical_memory_offset: VirtAddr,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>
) -> Option<PhysFrame> {
    let new_pml4_frame = frame_allocator.allocate_frame()?;
    let new_pml4_addr = physical_memory_offset + new_pml4_frame.start_address().as_u64();
    let new_pml4: &mut PageTable = &mut *new_pml4_addr.as_mut_ptr();

    new_pml4.zero();

    let active_pml4 = active_level_4_table(physical_memory_offset);

    // Higher half is 256..512 in x86_64
    for i in 256..512 {
        new_pml4[i] = active_pml4[i].clone();
    }

    Some(new_pml4_frame)
}

#[allow(dead_code)]
pub unsafe fn init_mapper_from_frame(
    pml4_frame: PhysFrame,
    physical_memory_offset: VirtAddr
) -> OffsetPageTable<'static> {
    let pml4_addr = physical_memory_offset + pml4_frame.start_address().as_u64();
    let pml4: &mut PageTable = &mut *pml4_addr.as_mut_ptr();
    OffsetPageTable::new(pml4, physical_memory_offset)
}
