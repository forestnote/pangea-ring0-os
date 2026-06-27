use spin::Mutex;

pub const PAGE_SIZE: usize = 4096;
pub const LIMINE_MEMMAP_USABLE: u64 = 0;

pub struct PageFrameAllocator {
    bitmap_ptr: *mut u8,
    total_pages: usize,
    usable_pages: usize,
    used_pages: usize,
    last_scanned_page: usize,
}

unsafe impl Send for PageFrameAllocator {}
unsafe impl Sync for PageFrameAllocator {}

pub static PMM: Mutex<Option<PageFrameAllocator>> = Mutex::new(None);

impl PageFrameAllocator {
    pub fn init(mem_map: &[&limine::memmap::Entry], hhdm_offset: u64) {
        let mut max_addr = 0;
        for entry in mem_map {
            let end = entry.base + entry.length;
            if end > max_addr { max_addr = end; }
        }

        let total_pages = (max_addr as usize) / PAGE_SIZE;
        let bitmap_size = align_up(total_pages / 8, PAGE_SIZE);

        let mut bitmap_phys_addr = 0;
        for entry in mem_map {
            if entry.type_ == LIMINE_MEMMAP_USABLE && entry.length as usize >= bitmap_size {
                bitmap_phys_addr = entry.base;
                break;
            }
        }

        if bitmap_phys_addr == 0 {
            panic!("[ FATAL ] Not enough contiguous physical memory for PMM bitmap!");
        }

        let bitmap_virt_addr = (bitmap_phys_addr + hhdm_offset) as *mut u8;

        unsafe {
            core::ptr::write_bytes(bitmap_virt_addr, 0xFF, bitmap_size);
        }

        let mut allocator = PageFrameAllocator {
            bitmap_ptr: bitmap_virt_addr,
            total_pages,
            usable_pages: 0,
            used_pages: 0,
            last_scanned_page: 0,
        };

        for entry in mem_map {
            if entry.type_ == LIMINE_MEMMAP_USABLE {
                let start_page = (entry.base as usize) / PAGE_SIZE;
                let end_page = start_page + ((entry.length as usize) / PAGE_SIZE);

                for i in start_page..end_page {
                    allocator.free_frame_internal(i);
                    allocator.usable_pages += 1;
                }
            }
        }

        let bitmap_start_page = (bitmap_phys_addr as usize) / PAGE_SIZE;
        let bitmap_end_page = bitmap_start_page + (bitmap_size / PAGE_SIZE);
        for i in bitmap_start_page..bitmap_end_page {
            allocator.lock_frame_internal(i);
            allocator.usable_pages -= 1;
        }

        *PMM.lock() = Some(allocator);
    }

    fn free_frame_internal(&mut self, page_idx: usize) {
        let byte_idx = page_idx / 8;
        let bit_idx = page_idx % 8;
        unsafe {
            let byte = self.bitmap_ptr.add(byte_idx);
            *byte &= !(1 << bit_idx);
        }
    }

    fn lock_frame_internal(&mut self, page_idx: usize) {
        let byte_idx = page_idx / 8;
        let bit_idx = page_idx % 8;
        unsafe {
            let byte = self.bitmap_ptr.add(byte_idx);
            *byte |= 1 << bit_idx;
        }
    }

    pub fn allocate_frame(&mut self) -> Option<usize> {
        for i in 0..self.total_pages {
            let page_idx = (self.last_scanned_page + i) % self.total_pages;
            let byte_idx = page_idx / 8;
            let bit_idx = page_idx % 8;

            unsafe {
                let byte = self.bitmap_ptr.add(byte_idx);
                if (*byte & (1 << bit_idx)) == 0 {
                    *byte |= 1 << bit_idx;
                    self.last_scanned_page = page_idx;
                    self.used_pages += 1;
                    return Some(page_idx * PAGE_SIZE);
                }
            }
        }
        None
    }

    // 警告対策: 将来のメモリ解放機能のために維持
    #[allow(dead_code)]
    pub fn free_frame(&mut self, phys_addr: usize) {
        let page_idx = phys_addr / PAGE_SIZE;
        self.free_frame_internal(page_idx);
        self.used_pages -= 1;
    }

    pub fn get_usable_ram_mb(&self) -> usize {
        (self.usable_pages * PAGE_SIZE) / (1024 * 1024)
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

unsafe impl FrameAllocator<Size4KiB> for PageFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if let Some(phys_addr) = self.allocate_frame() {
            Some(PhysFrame::containing_address(PhysAddr::new(phys_addr as u64)))
        } else {
            None
        }
    }
}
