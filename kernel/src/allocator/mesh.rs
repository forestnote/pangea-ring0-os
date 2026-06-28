use core::ptr::null_mut;
use core::alloc::Layout;

const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

struct ListNode {
    next: Option<&'static mut ListNode>,
}

pub struct MeshAllocator {
    list_heads: [Option<&'static mut ListNode>; 9],
    fallback_allocator: BumpAllocator,
}

impl MeshAllocator {
    pub const fn new() -> Self {
        MeshAllocator {
            list_heads: [None, None, None, None, None, None, None, None, None],
            fallback_allocator: BumpAllocator::new(),
        }
    }

    pub fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.fallback_allocator.init(heap_start, heap_size);
    }

    fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
        self.fallback_allocator.alloc(layout)
    }

    fn size_class_index(size: usize) -> Option<usize> {
        BLOCK_SIZES.iter().position(|&s| s >= size)
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let required_size = layout.size().max(layout.align());
        let index_opt = Self::size_class_index(required_size);

        if let Some(index) = index_opt {
            let size = BLOCK_SIZES[index];
            if let Some(node) = self.list_heads[index].take() {
                self.list_heads[index] = node.next.take();
                return node as *mut ListNode as *mut u8;
            }

            // Fallback: allocate a new block of size `size` with alignment `size`
            // This guarantees that the block pointer itself is aligned to `size`.
            // Since `size >= layout.align()`, the pointer will be correctly aligned.
            let block_align = size; 
            let block_layout = Layout::from_size_align(size, block_align).unwrap();
            self.fallback_alloc(block_layout)
        } else {
            // Large allocation
            self.fallback_alloc(layout)
        }
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let required_size = layout.size().max(layout.align());
        let index_opt = Self::size_class_index(required_size);

        if let Some(index) = index_opt {
            // Push to free list
            let node_ptr = ptr as *mut ListNode;
            node_ptr.write(ListNode {
                next: self.list_heads[index].take(),
            });
            self.list_heads[index] = Some(&mut *node_ptr);
        } else {
            // For now, large allocations (>2048) are leaked.
            // A True Mesh allocator would unmap the page or return it to PMM.
        }
    }
}

pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
}

impl BumpAllocator {
    pub const fn new() -> Self {
        BumpAllocator {
            heap_start: 0,
            heap_end: 0,
            next: 0,
        }
    }

    pub fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_start = heap_start;
        self.heap_end = heap_start + heap_size;
        self.next = heap_start;
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let alloc_start = align_up(self.next, layout.align());
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return null_mut(),
        };

        if alloc_end <= self.heap_end {
            self.next = alloc_end;
            alloc_start as *mut u8
        } else {
            null_mut()
        }
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
