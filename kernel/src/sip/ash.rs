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
    JeqFwd(Reg, Reg, u8),
    LoadContext(Reg, usize),
    Exit,
}

pub struct AshContext { pub data: [u8; 64] }

pub struct AshVm { registers: [u64; 6] }

impl AshVm {
    pub fn new() -> Self { AshVm { registers: [0; 6] } }

    pub fn execute(&mut self, instructions: &[Instruction], context: &AshContext) -> u64 {
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
                Instruction::JeqFwd(dst, src, offset) => {
                    if self.registers[dst as usize] == self.registers[src as usize] {
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
                Instruction::LoadContext(dst, offset) => {
                    let d = Self::reg_to_x86(dst);
                    if offset < 64 {
                        code.push(0x48); code.push(0x0f); code.push(0xb6); code.push(0x40 | (d << 3) | 0x07); code.push(offset as u8);
                    } else {
                        code.push(0x48); code.push(0xc7); code.push(0xc0 + d); code.extend_from_slice(&[0, 0, 0, 0]);
                    }
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

    pub unsafe fn execute(&self, context: &AshContext) -> u64 {
        let func: extern "C" fn(*const AshContext) -> u64 = core::mem::transmute(self.buffer);
        func(context as *const _)
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
