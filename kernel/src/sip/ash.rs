use alloc::vec::Vec;
use alloc::alloc::{alloc, dealloc, Layout};
use core::ptr;
use x86_64::VirtAddr;
use x86_64::structures::paging::{Page, PageTableFlags, Mapper, Size4KiB};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg { R0, R1, R2, R3, R4, R5 }

#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    LoadImm(Reg, u64),
    Add(Reg, Reg),
    Sub(Reg, Reg),
    And(Reg, Reg),
    Or(Reg, Reg),
    Xor(Reg, Reg),
    Shl(Reg, u8),
    Shr(Reg, u8),
    JeqFwd(Reg, Reg, u8),
    JneFwd(Reg, Reg, u8),
    JltFwd(Reg, Reg, u8),
    LoadContext(Reg, usize),
    StoreContext(Reg, usize),
    LoadDyn(Reg, Reg),
    StoreDyn(Reg, Reg),
    LoadState(Reg, u8),
    StoreState(Reg, u8),
    LoadStateDyn(Reg, Reg),
    StoreStateDyn(Reg, Reg),
    LoadNet32(Reg, Reg),
    LoopBwd(Reg, u8),
    Exit,
}

pub struct AshContext {
    pub data: [u8; 64],
    pub state: [u64; 8],
}

pub struct AshVm { registers: [u64; 6] }

impl AshVm {
    pub fn new() -> Self { AshVm { registers: [0; 6] } }

    pub fn execute(&mut self, instructions: &[Instruction], context: &mut AshContext) -> u64 {
        self.registers.fill(0);
        let mut pc = 0;
        while pc < instructions.len() {
            let instr = instructions[pc];
            pc += 1;
            match instr {
                Instruction::LoadImm(reg, val) => self.registers[reg as usize] = val,
                Instruction::Add(dst, src) => self.registers[dst as usize] = self.registers[dst as usize].wrapping_add(self.registers[src as usize]),
                Instruction::Sub(dst, src) => self.registers[dst as usize] = self.registers[dst as usize].wrapping_sub(self.registers[src as usize]),
                Instruction::And(dst, src) => self.registers[dst as usize] &= self.registers[src as usize],
                Instruction::Or(dst, src) => self.registers[dst as usize] |= self.registers[src as usize],
                Instruction::Xor(dst, src) => self.registers[dst as usize] ^= self.registers[src as usize],
                Instruction::Shl(dst, shift) => self.registers[dst as usize] <<= shift,
                Instruction::Shr(dst, shift) => self.registers[dst as usize] >>= shift,
                Instruction::JeqFwd(dst, src, offset) => {
                    if self.registers[dst as usize] == self.registers[src as usize] {
                        pc += offset as usize;
                    }
                }
                Instruction::JneFwd(dst, src, offset) => {
                    if self.registers[dst as usize] != self.registers[src as usize] {
                        pc += offset as usize;
                    }
                }
                Instruction::JltFwd(dst, src, offset) => {
                    if self.registers[dst as usize] < self.registers[src as usize] {
                        pc += offset as usize;
                    }
                }
                Instruction::LoadContext(dst, offset) => {
                    if offset < context.data.len() {
                        self.registers[dst as usize] = context.data[offset] as u64;
                    } else {
                        self.registers[dst as usize] = 0;
                    }
                }
                Instruction::StoreContext(data, offset) => {
                    if offset < context.data.len() {
                        context.data[offset] = self.registers[data as usize] as u8;
                    }
                }
                Instruction::LoadDyn(dst, src) => {
                    let off = (self.registers[src as usize] & 0x3F) as usize;
                    self.registers[dst as usize] = context.data[off] as u64;
                }
                Instruction::StoreDyn(data, off_reg) => {
                    let off = (self.registers[off_reg as usize] & 0x3F) as usize;
                    context.data[off] = self.registers[data as usize] as u8;
                }
                Instruction::LoadState(dst, offset) => {
                    if offset < 8 {
                        self.registers[dst as usize] = context.state[offset as usize];
                    }
                }
                Instruction::StoreState(src, offset) => {
                    if offset < 8 {
                        context.state[offset as usize] = self.registers[src as usize];
                    }
                }
                Instruction::LoadStateDyn(dst, off_reg) => {
                    let off = (self.registers[off_reg as usize] & 0x07) as usize;
                    self.registers[dst as usize] = context.state[off];
                }
                Instruction::StoreStateDyn(data, off_reg) => {
                    let off = (self.registers[off_reg as usize] & 0x07) as usize;
                    context.state[off] = self.registers[data as usize];
                }
                Instruction::LoadNet32(dst, off_reg) => {
                    let off = (self.registers[*off_reg as usize] & 0x3C) as usize;
                    let b0 = context.data[off] as u64;
                    let b1 = context.data[off + 1] as u64;
                    let b2 = context.data[off + 2] as u64;
                    let b3 = context.data[off + 3] as u64;
                    self.registers[*dst as usize] = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
                }
                Instruction::LoopBwd(r, count) => {
                    self.registers[*r as usize] = self.registers[*r as usize].wrapping_sub(1);
                    if self.registers[*r as usize] != 0 {
                        pc = pc.saturating_sub(*count as usize + 1);
                    }
                }
                Instruction::Exit => break,
            }
        }
        self.registers[Reg::R0 as usize]
    }
}

// ==========================================
// ★ The JIT Compiler with W^X Enforcer
// ==========================================
pub struct AshJit {
    buffer: *mut u8,
    layout: Layout,
    len: usize,
}

impl AshJit {
    pub fn new() -> Self {
        // 4KB(4096バイト)境界にアライメントされた専用ページをアロケータから強奪する。
        // これにより、このページをRead-Onlyに変更しても他のヒープデータに影響が及ばない。
        let layout = Layout::from_size_align(4096, 4096).unwrap();
        let buffer = unsafe { alloc(layout) };
        AshJit { buffer, layout, len: 0 }
    }

    fn reg_to_x86(reg: Reg) -> u8 {
        match reg { Reg::R0=>0, Reg::R1=>1, Reg::R2=>2, Reg::R3=>3, Reg::R4=>5, Reg::R5=>6 }
    }

    pub fn compile(&mut self, instructions: &[Instruction]) {
        let mut code = Vec::new();

        // Prologue
        code.push(0x53); // push rbx
        code.push(0x55); // push rbp
        code.extend_from_slice(&[0x31, 0xc0]); // xor eax, eax
        code.extend_from_slice(&[0x31, 0xc9]); // xor ecx, ecx
        code.extend_from_slice(&[0x31, 0xd2]); // xor edx, edx
        code.extend_from_slice(&[0x31, 0xdb]); // xor ebx, ebx
        code.extend_from_slice(&[0x31, 0xed]); // xor ebp, ebp
        code.extend_from_slice(&[0x31, 0xf6]); // xor esi, esi

        // Body
        let mut instr_offsets = Vec::with_capacity(instructions.len() + 1);
        let mut backpatch_list = Vec::new();

        for (i, &instr) in instructions.iter().enumerate() {
            instr_offsets.push(code.len());
            match instr {
                Instruction::LoadImm(reg, val) => {
                    let dst = Self::reg_to_x86(reg);
                    code.push(0x48); code.push(0xb8 + dst); code.extend_from_slice(&val.to_le_bytes());
                }
                Instruction::Add(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    code.push(0x48); code.push(0x01); code.push(0xc0 | (s << 3) | d);
                }
                Instruction::Sub(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    code.push(0x48); code.push(0x29); code.push(0xc0 | (s << 3) | d);
                }
                Instruction::And(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    code.push(0x48); code.push(0x21); code.push(0xc0 | (s << 3) | d);
                }
                Instruction::Or(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    code.push(0x48); code.push(0x09); code.push(0xc0 | (s << 3) | d);
                }
                Instruction::Xor(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    code.push(0x48); code.push(0x31); code.push(0xc0 | (s << 3) | d);
                }
                Instruction::Shl(dst, shift) => {
                    let d = Self::reg_to_x86(dst);
                    code.push(0x48); code.push(0xc1); code.push(0xe0 | d); code.push(shift);
                }
                Instruction::Shr(dst, shift) => {
                    let d = Self::reg_to_x86(dst);
                    code.push(0x48); code.push(0xc1); code.push(0xe8 | d); code.push(shift);
                }
                Instruction::JeqFwd(dst, src, offset) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    // cmp dst, src
                    code.push(0x48); code.push(0x39); code.push(0xc0 | (s << 3) | d);
                    // je rel32
                    code.push(0x0f); code.push(0x84);
                    let patch_offset = code.len();
                    code.extend_from_slice(&[0, 0, 0, 0]); // dummy offset
                    backpatch_list.push((patch_offset, i + 1 + offset as usize));
                }
                Instruction::JneFwd(dst, src, offset) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    // cmp dst, src
                    code.push(0x48); code.push(0x39); code.push(0xc0 | (s << 3) | d);
                    // jne rel32
                    code.push(0x0f); code.push(0x85);
                    let patch_offset = code.len();
                    code.extend_from_slice(&[0, 0, 0, 0]); // dummy offset
                    backpatch_list.push((patch_offset, i + 1 + offset as usize));
                }
                Instruction::JltFwd(dst, src, offset) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    // cmp dst, src
                    code.push(0x48); code.push(0x39); code.push(0xc0 | (s << 3) | d);
                    // jb rel32 (unsigned less than)
                    code.push(0x0f); code.push(0x82);
                    let patch_offset = code.len();
                    code.extend_from_slice(&[0, 0, 0, 0]); // dummy offset
                    backpatch_list.push((patch_offset, i + 1 + offset as usize));
                }
                Instruction::LoadContext(dst, offset) => {
                    let d = Self::reg_to_x86(dst);
                    if offset < 64 {
                        code.push(0x48); code.push(0x0f); code.push(0xb6); code.push(0x40 | (d << 3) | 0x07); code.push(offset as u8);
                    } else {
                        code.push(0x48); code.push(0xc7); code.push(0xc0 + d); code.extend_from_slice(&[0, 0, 0, 0]);
                    }
                }
                Instruction::StoreContext(data_reg, offset) => {
                    let d = Self::reg_to_x86(data_reg);
                    if offset < 64 {
                        // mov [rdi + disp8], reg8
                        code.push(0x40); code.push(0x88); code.push(0x40 | (d << 3) | 0x07); code.push(offset as u8);
                    }
                }
                Instruction::LoadDyn(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    // and src, 63 (Zero-cost branchless bounds check)
                    code.push(0x48); code.push(0x83); code.push(0xe0 | s); code.push(0x3F);
                    // movzx dst, byte ptr [rdi + src]
                    code.push(0x48); code.push(0x0f); code.push(0xb6);
                    code.push(0x04 | (d << 3));
                    code.push((s << 3) | 7);
                }
                Instruction::StoreDyn(data_reg, off_reg) => {
                    let data = Self::reg_to_x86(data_reg); let off = Self::reg_to_x86(off_reg);
                    // and off, 63
                    code.push(0x48); code.push(0x83); code.push(0xe0 | off); code.push(0x3F);
                    // mov byte ptr [rdi + off], data
                    code.push(0x40); code.push(0x88);
                    code.push(0x04 | (data << 3));
                    code.push((off << 3) | 7);
                }
                Instruction::LoadState(dst, offset) => {
                    let d = Self::reg_to_x86(dst);
                    if offset < 8 {
                        let disp = 64 + offset * 8;
                        // mov r64, qword ptr [rdi + disp8]
                        code.push(0x48); code.push(0x8b); code.push(0x40 | (d << 3) | 0x07); code.push(disp as u8);
                    }
                }
                Instruction::StoreState(src, offset) => {
                    let s = Self::reg_to_x86(src);
                    if offset < 8 {
                        let disp = 64 + offset * 8;
                        // mov qword ptr [rdi + disp8], r64
                        code.push(0x48); code.push(0x89); code.push(0x40 | (s << 3) | 0x07); code.push(disp as u8);
                    }
                }
                Instruction::LoadStateDyn(dst, off_reg) => {
                    let d = Self::reg_to_x86(dst); let off = Self::reg_to_x86(off_reg);
                    // and off_reg, 7
                    code.push(0x48); code.push(0x83); code.push(0xe0 | off); code.push(0x07);
                    // mov dst, qword ptr [rdi + off_reg * 8 + 64]
                    code.push(0x48); code.push(0x8b);
                    code.push(0x40 | (d << 3) | 0x04); // mod=01, reg=d, rm=100
                    code.push(0xc0 | (off << 3) | 0x07); // SIB: scale=8, index=off, base=rdi
                    code.push(64); // disp8 = 64
                }
                Instruction::StoreStateDyn(data_reg, off_reg) => {
                    let data = Self::reg_to_x86(data_reg); let off = Self::reg_to_x86(off_reg);
                    // and off_reg, 7
                    code.push(0x48); code.push(0x83); code.push(0xe0 | off); code.push(0x07);
                    // mov qword ptr [rdi + off_reg * 8 + 64], data
                    code.push(0x48); code.push(0x89);
                    code.push(0x40 | (data << 3) | 0x04); // mod=01, reg=data, rm=100
                    code.push(0xc0 | (off << 3) | 0x07); // SIB: scale=8, index=off, base=rdi
                    code.push(64); // disp8 = 64
                }
                Instruction::LoadNet32(dst, off_reg) => {
                    let d = Self::reg_to_x86(*dst); let off = Self::reg_to_x86(*off_reg);
                    // and off_reg, 60 (0x3C)
                    code.push(0x48); code.push(0x83); code.push(0xe0 | off); code.push(0x3C);
                    // mov r32, dword ptr [rdi + off_reg]
                    code.push(0x8b);
                    code.push(0x04 | (d << 3)); // mod=00, reg=d, rm=100
                    code.push((off << 3) | 7); // SIB: scale=1, index=off, base=rdi
                    // bswap r32
                    code.push(0x0f); code.push(0xc8 | d);
                }
                Instruction::LoopBwd(r, count) => {
                    let target_idx = i.saturating_sub(*count as usize);
                    let target_offset = instr_offsets[target_idx];
                    let reg = Self::reg_to_x86(*r);
                    // dec reg (64-bit)
                    code.push(0x48); code.push(0xFF); code.push(0xC8 | reg);
                    // jnz rel8
                    let current_len_after = code.len() + 2;
                    let rel8 = target_offset as isize - current_len_after as isize;
                    assert!(rel8 >= -128 && rel8 <= 127, "LoopBwd target too far");
                    code.push(0x75);
                    code.push(rel8 as u8);
                }
                Instruction::Exit => {
                    code.push(0x5d); // pop rbp
                    code.push(0x5b); // pop rbx
                    code.push(0xc3); // ret
                }
            }
        }
        instr_offsets.push(code.len());

        // Backpatching forward jumps
        for (patch_offset, target_idx) in backpatch_list {
            let safe_target = if target_idx >= instructions.len() {
                instructions.len() - 1 // Fallback to last instruction (should be Exit)
            } else {
                target_idx
            };
            let target_byte_offset = instr_offsets[safe_target];
            let jump_end = patch_offset + 4;
            let rel32 = (target_byte_offset as isize - jump_end as isize) as i32;
            let bytes = rel32.to_le_bytes();
            code[patch_offset] = bytes[0];
            code[patch_offset+1] = bytes[1];
            code[patch_offset+2] = bytes[2];
            code[patch_offset+3] = bytes[3];
        }

        // バイトコードを一時領域から専用の4KBページバッファへ物理コピーする
        unsafe { ptr::copy_nonoverlapping(code.as_ptr(), self.buffer, code.len()); }
        self.len = code.len();
    }

    /// W^X Enforcer: マシン語書き込み完了後、ページを「実行可能・書き込み不可 (RX)」へフリップする
    pub fn seal(&self) {
        let mut vmm_guard = crate::CORE_VMMS[crate::apic::lapic_id() as usize].lock();
        let mapper = vmm_guard.as_mut().expect("VMM not initialized");
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(self.buffer as u64));

        // PRESENT ビットのみを立てる（WRITABLE と NO_EXECUTE を物理的に剥奪する）
        let flags = PageTableFlags::PRESENT;
        unsafe {
            let flush = mapper.update_flags(page, flags).expect("Failed to seal JIT page");
            flush.flush(); // TLBをフラッシュし、CPUに新たなセキュリティ境界を強制認識させる
        }
    }

    pub unsafe fn execute(&self, context: &mut AshContext) -> u64 {
        let func: extern "C" fn(*mut AshContext) -> u64 = core::mem::transmute(self.buffer);
        func(context as *mut _)
    }
}

/// JITエンジンが破棄される際の浄化機構
impl Drop for AshJit {
    fn drop(&mut self) {
        // [ 警告 ]
        // この処理を怠ると、Read-Only(RX)になったページがグローバルアロケータに返却される。
        // その後、別のプロセスがこのページを再利用してデータ(RW)を書き込もうとした瞬間、
        // ページフォルトが炸裂してOSが即死する。確実に「RW+NX」へリストアしなければならない。
        let mut vmm_guard = crate::CORE_VMMS[crate::apic::lapic_id() as usize].lock();
        if let Some(mapper) = vmm_guard.as_mut() {
            let page = Page::<Size4KiB>::containing_address(VirtAddr::new(self.buffer as u64));
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
            unsafe {
                if let Ok(flush) = mapper.update_flags(page, flags) {
                    flush.flush(); // TLBフラッシュ
                }
            }
        }

        // ページ属性を浄化した後、安全にメモリをアロケータへ返却する
        unsafe { dealloc(self.buffer, self.layout); }
    }
}
