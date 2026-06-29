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
    CallExt(u8),
    Exit,
}

pub extern "C" fn helper_get_time() -> u64 {
    // [ Intel IBT (Indirect Branch Tracking) ]
    // Called indirectly from JIT, must start with ENDBR64
    unsafe { core::arch::asm!("endbr64") };
    unsafe { core::arch::x86_64::_rdtsc() }
}

pub extern "C" fn helper_debug_print(val: u64) -> u64 {
    // [ Intel IBT (Indirect Branch Tracking) ]
    unsafe { core::arch::asm!("endbr64") };
    crate::serial_println!("[ ASH JIT LOG ] Value: \t{:#x} ({})", val, val);
    0
}

pub extern "C" fn helper_sls_map(oid: u64, token: u64) -> u64 {
    // [ Intel IBT (Indirect Branch Tracking) ]
    unsafe { core::arch::asm!("endbr64") };
    
    if !crate::sls::verify_capability(crate::sls::ObjectId(oid), token) {
        crate::serial_println!("[ SECURITY ] Invalid SLS Capability Token for OID {:#x}! Access Denied.", oid);
        return 0;
    }
    
    crate::sls::get_object(crate::sls::ObjectId(oid)).unwrap_or(0)
}

// --- CHERI (Capability Hardware Enhanced RISC Instructions) Concept ---
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Perms(pub u8);

impl Perms {
    pub const NONE: Perms  = Perms(0);
    pub const READ: Perms  = Perms(1 << 0);
    pub const WRITE: Perms = Perms(1 << 1);
    pub const EXEC: Perms  = Perms(1 << 2);
    pub const RW: Perms    = Perms(Self::READ.0 | Self::WRITE.0);

    pub fn contains(&self, other: Perms) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// A software-emulated CHERI-style capability.
/// Carries an explicit memory boundary (mask) and permissions, 
/// which the JIT uses to enforce bounds in hardware natively.
#[repr(C)]
pub struct CheriCap<T> {
    pub base: *mut T,
    pub mask: usize, // Must be (power of 2) - 1
    pub perms: Perms,
    _pad: [u8; 7],
}

impl<T> CheriCap<T> {
    pub unsafe fn new_root(ptr: *mut T, len: usize, perms: Perms) -> Self {
        assert!(len.is_power_of_two(), "CheriCap length must be a power of two for MBC");
        Self {
            base: ptr,
            mask: len - 1,
            perms,
            _pad: [0; 7],
        }
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if !self.perms.contains(Perms::READ) || index > self.mask {
            return None;
        }
        Some(unsafe { &*self.base.add(index) })
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if !self.perms.contains(Perms::WRITE) || index > self.mask {
            return None;
        }
        Some(unsafe { &mut *self.base.add(index) })
    }
}

#[repr(C)]
pub struct AshContext {
    pub memory: CheriCap<u8>, // Offset 0
    pub state: CheriCap<u64>, // Offset 24
}

#[derive(Clone)]
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
                    self.registers[dst as usize] = *context.memory.get(offset).unwrap_or(&0) as u64;
                }
                Instruction::StoreContext(data, offset) => {
                    if let Some(byte) = context.memory.get_mut(offset) {
                        *byte = self.registers[data as usize] as u8;
                    }
                }
                Instruction::LoadDyn(dst, src) => {
                    let off = (self.registers[src as usize] as usize) & context.memory.mask;
                    self.registers[dst as usize] = *context.memory.get(off).unwrap_or(&0) as u64;
                }
                Instruction::StoreDyn(data, off_reg) => {
                    let off = (self.registers[off_reg as usize] as usize) & context.memory.mask;
                    if let Some(byte) = context.memory.get_mut(off) {
                        *byte = self.registers[data as usize] as u8;
                    }
                }
                Instruction::LoadState(dst, offset) => {
                    self.registers[dst as usize] = *context.state.get(offset as usize).unwrap_or(&0);
                }
                Instruction::StoreState(src, offset) => {
                    if let Some(val) = context.state.get_mut(offset as usize) {
                        *val = self.registers[src as usize];
                    }
                }
                Instruction::LoadStateDyn(dst, off_reg) => {
                    let off = (self.registers[off_reg as usize] as usize) & context.state.mask;
                    self.registers[dst as usize] = *context.state.get(off).unwrap_or(&0);
                }
                Instruction::StoreStateDyn(data, off_reg) => {
                    let off = (self.registers[off_reg as usize] as usize) & context.state.mask;
                    if let Some(val) = context.state.get_mut(off) {
                        *val = self.registers[data as usize];
                    }
                }
                Instruction::LoadNet32(dst, off_reg) => {
                    let off = (self.registers[off_reg as usize] as usize) & context.memory.mask & !3;
                    let b0 = *context.memory.get(off).unwrap_or(&0) as u64;
                    let b1 = *context.memory.get(off + 1).unwrap_or(&0) as u64;
                    let b2 = *context.memory.get(off + 2).unwrap_or(&0) as u64;
                    let b3 = *context.memory.get(off + 3).unwrap_or(&0) as u64;
                    self.registers[dst as usize] = (b0 << 24) | (b1 << 16) | (b2 << 8) | b3;
                }
                Instruction::LoopBwd(r, count) => {
                    self.registers[r as usize] = self.registers[r as usize].wrapping_sub(1);
                    if self.registers[r as usize] != 0 {
                        pc = pc.saturating_sub(count as usize + 1);
                    }
                }
                Instruction::CallExt(func_id) => {
                    match func_id {
                        0 => {
                            self.registers[Reg::R0 as usize] = unsafe { core::arch::x86_64::_rdtsc() };
                        }
                        1 => {
                            crate::serial_println!("[ ASH JIT LOG ] Value: {:#10x} ({})", self.registers[Reg::R1 as usize], self.registers[Reg::R1 as usize]);
                            self.registers[Reg::R0 as usize] = 0;
                        }
                        _ => {}
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
    blind_key: u64, // JIT Spraying Defense (Constant Blinding Key)
}

impl AshJit {
    pub fn new() -> Self {
        // 4KB(4096バイト)境界にアライメントされた専用ページをアロケータから強奪する。
        // これにより、このページをRead-Onlyに変更しても他のヒープデータに影響が及ばない。
        let layout = Layout::from_size_align(4096, 4096).unwrap();
        let buffer = unsafe { alloc(layout) };
        // JIT Spraying攻撃を防ぐための乱数キー（簡易的にTSCを使用）
        let blind_key = unsafe { core::arch::x86_64::_rdtsc() };
        AshJit { buffer, layout, len: 0, blind_key }
    }

    fn reg_to_x86(reg: Reg) -> u8 {
        match reg { Reg::R0=>0, Reg::R1=>1, Reg::R2=>2, Reg::R3=>3, Reg::R4=>5, Reg::R5=>6 }
    }

    pub fn compile(&mut self, instructions: &[Instruction]) -> Result<(), &'static str> {
        // [ eBPF-style Static Verifier ]
        // コンパイル前に命令グラフを静的解析し、安全性（停止性、不正ジャンプ）を証明する
        Self::verify_bytecode(instructions).expect("Ash Verifier rejected bytecode: Unsafe execution detected!");

        let mut code = Vec::new();

        // [ Intel IBT (Indirect Branch Tracking) Defense ]
        // 間接ジャンプ（execute_safeによる呼び出し）の着地点として ENDBR64 を強制配置
        // これにより、CET有効下でのJOP (Jump-Oriented Programming) を完全に無力化
        code.push(0xf3); code.push(0x0f); code.push(0x1e); code.push(0xfa);

        // Prologue
        code.push(0x53); // push rbx
        code.push(0x55); // push rbp
        code.extend_from_slice(&[0x31, 0xc0]); // xor eax, eax
        code.extend_from_slice(&[0x31, 0xc9]); // xor ecx, ecx
        code.extend_from_slice(&[0x31, 0xd2]); // xor edx, edx
        code.extend_from_slice(&[0x31, 0xdb]); // xor ebx, ebx
        code.extend_from_slice(&[0x31, 0xed]); // xor ebp, ebp
        code.extend_from_slice(&[0x31, 0xf6]); // xor esi, esi
        
        // Initialize Gas Counter (r9 = 10000)
        code.push(0x49); code.push(0xc7); code.push(0xc1); 
        code.extend_from_slice(&10000u32.to_le_bytes());

        // Body
        let mut instr_offsets = Vec::with_capacity(instructions.len() + 1);
        let mut backpatch_list = Vec::new();

        for (i, &instr) in instructions.iter().enumerate() {
            instr_offsets.push(code.len());
            match instr {
                Instruction::LoadImm(reg, val) => {
                    let dst = Self::reg_to_x86(reg);
                    // 【Constant Blinding】
                    // 攻撃者が LoadImm を用いてシェルコードを JIT メモリに埋め込む "JIT Spraying" を防ぐため、
                    // 即値を乱数キーで XOR 難読化してからメモリに書き込み、実行時に復元する。
                    let blinded = val ^ self.blind_key;
                    // mov dst, blinded
                    code.push(0x48); code.push(0xb8 + dst); code.extend_from_slice(&blinded.to_le_bytes());
                    // mov r8, blind_key
                    code.push(0x49); code.push(0xb8); code.extend_from_slice(&self.blind_key.to_le_bytes());
                    // xor dst, r8
                    code.push(0x4c); code.push(0x31); code.push(0xc0 | dst);
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
                    // r8 = offset
                    code.push(0x49); code.push(0xc7); code.push(0xc0); code.extend_from_slice(&(offset as u32).to_le_bytes());
                    // r11 = memory.mask (offset 8)
                    code.push(0x4c); code.push(0x8b); code.push(0x5f); code.push(0x08);
                    // and r8, r11
                    code.push(0x4d); code.push(0x21); code.push(0xd8);
                    // lfence (Speculative Load Hardening against Spectre v1)
                    code.push(0x0f); code.push(0xae); code.push(0xe8);
                    // r10 = memory.base (offset 0)
                    code.push(0x4c); code.push(0x8b); code.push(0x17);
                    // movzx dst, byte ptr [r10 + r8]
                    code.push(0x4b); code.push(0x0f); code.push(0xb6); code.push((d << 3) | 0x04); code.push(0x02);
                }
                Instruction::StoreContext(data_reg, offset) => {
                    let s = Self::reg_to_x86(data_reg);
                    // r8 = offset
                    code.push(0x49); code.push(0xc7); code.push(0xc0); code.extend_from_slice(&(offset as u32).to_le_bytes());
                    // r11 = memory.mask
                    code.push(0x4c); code.push(0x8b); code.push(0x5f); code.push(0x08);
                    // and r8, r11
                    code.push(0x4d); code.push(0x21); code.push(0xd8);
                    // lfence (Speculative Load Hardening against Spectre v1)
                    code.push(0x0f); code.push(0xae); code.push(0xe8);
                    // r10 = memory.base
                    code.push(0x4c); code.push(0x8b); code.push(0x17);
                    // mov byte ptr [r10 + r8], src
                    code.push(0x43); code.push(0x88); code.push((s << 3) | 0x04); code.push(0x02);
                }
                Instruction::LoadDyn(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    // r8 = src
                    code.push(0x4d); code.push(0x89); code.push(0xc0 | s);
                    // r11 = memory.mask (offset 8)
                    code.push(0x4c); code.push(0x8b); code.push(0x5f); code.push(0x08);
                    // and r8, r11
                    code.push(0x4d); code.push(0x21); code.push(0xd8);
                    // lfence (Speculative Load Hardening against Spectre v1)
                    code.push(0x0f); code.push(0xae); code.push(0xe8);
                    // r10 = memory.base (offset 0)
                    code.push(0x4c); code.push(0x8b); code.push(0x17);
                    // movzx dst, byte ptr [r10 + r8]
                    code.push(0x4b); code.push(0x0f); code.push(0xb6); code.push((d << 3) | 0x04); code.push(0x02);
                }
                Instruction::StoreDyn(data_reg, off_reg) => {
                    let data = Self::reg_to_x86(data_reg); let off = Self::reg_to_x86(off_reg);
                    // r8 = off
                    code.push(0x4d); code.push(0x89); code.push(0xc0 | off);
                    // r11 = memory.mask (offset 8)
                    code.push(0x4c); code.push(0x8b); code.push(0x5f); code.push(0x08);
                    // and r8, r11
                    code.push(0x4d); code.push(0x21); code.push(0xd8);
                    // lfence (Speculative Load Hardening against Spectre v1)
                    code.push(0x0f); code.push(0xae); code.push(0xe8);
                    // r10 = memory.base (offset 0)
                    code.push(0x4c); code.push(0x8b); code.push(0x17);
                    // mov byte ptr [r10 + r8], data
                    code.push(0x43); code.push(0x88); code.push((data << 3) | 0x04); code.push(0x02);
                }
                Instruction::LoadState(dst, offset) => {
                    let d = Self::reg_to_x86(dst);
                    // r8 = offset
                    code.push(0x49); code.push(0xc7); code.push(0xc0); code.extend_from_slice(&(offset as u32).to_le_bytes());
                    // r11 = state.mask (offset 32)
                    code.push(0x4c); code.push(0x8b); code.push(0x5f); code.push(0x20);
                    // and r8, r11
                    code.push(0x4d); code.push(0x21); code.push(0xd8);
                    // lfence (Speculative Load Hardening against Spectre v1)
                    code.push(0x0f); code.push(0xae); code.push(0xe8);
                    // r10 = state.base (offset 24)
                    code.push(0x4c); code.push(0x8b); code.push(0x57); code.push(0x18);
                    // mov dst, qword ptr [r10 + r8 * 8]
                    code.push(0x4b); code.push(0x8b); code.push((d << 3) | 0x04); code.push(0xc2);
                }
                Instruction::StoreState(src, offset) => {
                    let s = Self::reg_to_x86(src);
                    // r8 = offset
                    code.push(0x49); code.push(0xc7); code.push(0xc0); code.extend_from_slice(&(offset as u32).to_le_bytes());
                    // r11 = state.mask (offset 32)
                    code.push(0x4c); code.push(0x8b); code.push(0x5f); code.push(0x20);
                    // and r8, r11
                    code.push(0x4d); code.push(0x21); code.push(0xd8);
                    // lfence (Speculative Load Hardening against Spectre v1)
                    code.push(0x0f); code.push(0xae); code.push(0xe8);
                    // r10 = state.base (offset 24)
                    code.push(0x4c); code.push(0x8b); code.push(0x57); code.push(0x18);
                    // mov qword ptr [r10 + r8 * 8], src
                    code.push(0x4b); code.push(0x89); code.push((s << 3) | 0x04); code.push(0xc2);
                }

                Instruction::LoadStateDyn(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    // r8 = src
                    code.push(0x4d); code.push(0x89); code.push(0xc0 | s);
                    // r11 = state.mask (offset 32)
                    code.push(0x4c); code.push(0x8b); code.push(0x5f); code.push(0x20);
                    // and r8, r11
                    code.push(0x4d); code.push(0x21); code.push(0xd8);
                    // lfence (Speculative Load Hardening against Spectre v1)
                    code.push(0x0f); code.push(0xae); code.push(0xe8);
                    // r10 = state.base (offset 24)
                    code.push(0x4c); code.push(0x8b); code.push(0x57); code.push(0x18);
                    // mov dst, qword ptr [r10 + r8 * 8]
                    code.push(0x4b); code.push(0x8b); code.push((d << 3) | 0x04); code.push(0xc2);
                }
                Instruction::StoreStateDyn(dst, src) => {
                    let d = Self::reg_to_x86(dst); let s = Self::reg_to_x86(src);
                    // r8 = dst
                    code.push(0x4d); code.push(0x89); code.push(0xc0 | d);
                    // r11 = state.mask
                    code.push(0x4c); code.push(0x8b); code.push(0x5f); code.push(0x20);
                    // and r8, r11
                    code.push(0x4d); code.push(0x21); code.push(0xd8);
                    // lfence (Speculative Load Hardening against Spectre v1)
                    code.push(0x0f); code.push(0xae); code.push(0xe8);
                    // r10 = state.base
                    code.push(0x4c); code.push(0x8b); code.push(0x57); code.push(0x18);
                    // mov qword ptr [r10 + r8 * 8], src
                    code.push(0x4b); code.push(0x89); code.push((s << 3) | 0x04); code.push(0xc2);
                }
                Instruction::LoadNet32(dst, off_reg) => {
                    let d = Self::reg_to_x86(dst); let off = Self::reg_to_x86(off_reg);
                    // mov r8, off
                    code.push(0x49); code.push(0x89); code.push(0xc0 | off);
                    // and r8, 60
                    code.push(0x49); code.push(0x83); code.push(0xe0); code.push(0x3C);
                    // mov r32, dword ptr [rdi + r8]
                    code.push(0x42); code.push(0x8b);
                    code.push(0x04 | (d << 3));
                    code.push(0x07);
                    // bswap r32
                    code.push(0x0f); code.push(0xc8 | d);
                }
                Instruction::LoopBwd(r, count) => {
                    let target_idx = i.saturating_sub(count as usize);
                    let target_offset = instr_offsets[target_idx];
                    let reg = Self::reg_to_x86(r);
                    
                    // GAS CHECK: dec r9
                    code.push(0x49); code.push(0xff); code.push(0xc9);
                    // jnz GAS_OK (skip next 5 bytes)
                    code.push(0x75); code.push(0x05);
                    // GAS_DEPLETED: return 0 early (xor eax, eax; pop rbp; pop rbx; ret)
                    code.push(0x31); code.push(0xc0);
                    code.push(0x5d);
                    code.push(0x5b);
                    code.push(0xc3);
                    // GAS_OK:
                    
                    // dec reg (64-bit)
                    code.push(0x48); code.push(0xFF); code.push(0xC8 | reg);
                    // jnz rel8
                    let current_len_after = code.len() + 2;
                    let rel8 = target_offset as isize - current_len_after as isize;
                    if rel8 < -128 || rel8 > 127 {
                        return Err("LoopBwd target too far (exceeds 8-bit relative jump limit)");
                    }
                    code.push(0x75);
                    code.push(rel8 as u8);
                }
                Instruction::CallExt(func_id) => {
                    // GAS CHECK for CallExt (Cost: 1000 Gas)
                    // FFI コールは重い処理であるため、大量に呼び出されて DoS になるのを防ぐ
                    // sub r9, 1000
                    code.push(0x49); code.push(0x81); code.push(0xe9); code.extend_from_slice(&1000u32.to_le_bytes());
                    // jns GAS_OK (skip next 5 bytes)
                    code.push(0x79); code.push(0x05);
                    // GAS_DEPLETED: return 0 early
                    code.push(0x31); code.push(0xc0);
                    code.push(0x5d);
                    code.push(0x5b);
                    code.push(0xc3);
                    // GAS_OK:

                    if func_id == 0 {
                        // helper_get_time (Returns u64 in rax)
                        // Push caller-saved registers: rdi, rcx, rdx, rsi, r9 (Gas counter).
                        // 5 registers = 40 bytes. Stack becomes 16-byte aligned.
                        code.push(0x57); // push rdi
                        code.push(0x51); // push rcx
                        code.push(0x52); // push rdx
                        code.push(0x56); // push rsi
                        code.push(0x41); code.push(0x51); // push r9

                        let addr = helper_get_time as *const () as usize;
                        code.push(0x49); code.push(0xbb); code.extend_from_slice(&addr.to_le_bytes()); // mov r11, addr
                        code.push(0x41); code.push(0xff); code.push(0xd3); // call r11

                        // Pop registers. DO NOT pop rax, as it holds the return value!
                        code.push(0x41); code.push(0x59); // pop r9
                        code.push(0x5e); // pop rsi
                        code.push(0x5a); // pop rdx
                        code.push(0x59); // pop rcx
                        code.push(0x5f); // pop rdi

                    } else if func_id == 1 {
                        // helper_debug_print (Takes arg in R1, returns nothing)
                        // Push caller-saved registers: rdi, rcx, rdx, rsi, rax, r9.
                        // And a dummy (r8) for 16-byte alignment (7 registers = 56 bytes).
                        code.push(0x57); // push rdi
                        code.push(0x51); // push rcx
                        code.push(0x52); // push rdx
                        code.push(0x56); // push rsi
                        code.push(0x50); // push rax
                        code.push(0x41); code.push(0x51); // push r9
                        code.push(0x41); code.push(0x50); // push r8 (dummy)

                        code.push(0x48); code.push(0x89); code.push(0xcf); // mov rdi, rcx (Pass R1 as first arg)

                        let addr = helper_debug_print as *const () as usize;
                        code.push(0x49); code.push(0xbb); code.extend_from_slice(&addr.to_le_bytes()); // mov r11, addr
                        code.push(0x41); code.push(0xff); code.push(0xd3); // call r11

                        code.push(0x41); code.push(0x58); // pop r8 (dummy)
                        code.push(0x41); code.push(0x59); // pop r9
                        code.push(0x58); // pop rax
                        code.push(0x5e); // pop rsi
                        code.push(0x5a); // pop rdx
                        code.push(0x59); // pop rcx
                        code.push(0x5f); // pop rdi
                    } else if func_id == 2 {
                        // helper_sls_map (Takes OID in R1, Token in R2, returns mapped address in R0)
                        code.push(0x57); // push rdi
                        code.push(0x51); // push rcx
                        code.push(0x52); // push rdx
                        code.push(0x56); // push rsi
                        code.push(0x41); code.push(0x51); // push r9

                        code.push(0x48); code.push(0x89); code.push(0xcf); // mov rdi, rcx (Pass R1 as first arg)
                        
                        let arg2 = Self::reg_to_x86(Reg::R2);
                        code.push(0x48); code.push(0x89); code.push(0xc6 | (arg2 << 3)); // mov rsi, reg[R2] (Pass R2 as second arg)

                        let addr = helper_sls_map as *const () as usize;
                        code.push(0x49); code.push(0xbb); code.extend_from_slice(&addr.to_le_bytes()); // mov r11, addr
                        code.push(0x41); code.push(0xff); code.push(0xd3); // call r11

                        // Pop registers
                        code.push(0x41); code.push(0x59); // pop r9
                        code.push(0x5e); // pop rsi
                        code.push(0x5a); // pop rdx
                        code.push(0x59); // pop rcx
                        code.push(0x5f); // pop rdi

                        // Move return value from rax to R0
                        let d = Self::reg_to_x86(Reg::R0);
                        code.push(0x49); code.push(0x89); code.push(0xc0 | d); // mov reg[R0], rax
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

        if code.len() > 4096 {
            return Err("JIT payload exceeds 4KB buffer bounds (Buffer Overflow Mitigated)");
        }

        // バイトコードを一時領域から専用の4KBページバッファへ物理コピーする
        unsafe { ptr::copy_nonoverlapping(code.as_ptr(), self.buffer, code.len()); }
        self.len = code.len();
        Ok(())
    }

    /// W^X Enforcer: マシン語書き込み完了後、ページを「実行可能・書き込み不可 (RX)」へフリップする
    pub fn seal(&self) {
        let mut vmm_guard = crate::CORE_VMMS[crate::apic::lapic_id() as usize].lock();
        let mapper = vmm_guard.as_mut().expect("VMM not initialized");
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(self.buffer as u64));

        // PRESENT ビットのみを立てる（WRITABLE と NO_EXECUTE を物理的に剥奪する）
        let flags = PageTableFlags::PRESENT;
        unsafe {
            let _flush = mapper.update_flags(page, flags).expect("Failed to seal JIT page");
            // TLBをフラッシュし、他のコアのTLBも確実にフラッシュする(Cross-Core TLB Shootdown)
            crate::smp::tlb_shootdown(self.buffer as u64);
        }
    }

    pub unsafe fn execute(&self, context: &mut AshContext) -> u64 {
        let func: extern "C" fn(*mut AshContext) -> u64 = core::mem::transmute(self.buffer);
        func(context as *mut _)
    }

    /// eBPF-style Static Verifier
    /// 実行前にバイトコードの安全性を静的に証明する
    fn verify_bytecode(instructions: &[Instruction]) -> Result<(), &'static str> {
        let len = instructions.len();
        if len == 0 { return Err("Empty bytecode"); }
        if !matches!(instructions[len - 1], Instruction::Exit) {
            return Err("Program must end with Exit instruction");
        }

        for (i, instr) in instructions.iter().enumerate() {
            match instr {
                Instruction::JeqFwd(_, _, offset) |
                Instruction::JneFwd(_, _, offset) |
                Instruction::JltFwd(_, _, offset) => {
                    if i + 1 + (*offset as usize) >= len {
                        return Err("Forward jump exceeds program bounds");
                    }
                }
                Instruction::LoopBwd(_, count) => {
                    if (*count as usize) > i {
                        return Err("Backward loop exceeds program bounds (underflow)");
                    }
                }
                Instruction::CallExt(id) => {
                    if *id > 2 {
                        return Err("Invalid helper function ID (Max: 2)");
                    }
                }
                _ => {}
            }
        }
        Ok(())
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
                if let Ok(_flush) = mapper.update_flags(page, flags) {
                    crate::smp::tlb_shootdown(self.buffer as u64); // Cross-Core TLB Shootdown
                }
            }
        }

        // ページ属性を浄化した後、安全にメモリをアロケータへ返却する
        unsafe { dealloc(self.buffer, self.layout); }
    }
}

use alloc::sync::Arc;

/// A pure Ring-0 Micro-Process based on the ASH JIT Sandbox.
pub struct AshProcess {
    memory_ptr: *mut u8,
    memory_size: usize,
    state_ptr: *mut u64,
    state_size: usize,
    jit: Arc<AshJit>,
    pub vm: AshVm,
    pub mpk_key: u8,
}

impl AshProcess {
    pub fn new(memory_size: usize, state_size: usize, jit: Arc<AshJit>, mpk_key: u8, hhdm_offset: u64) -> Self {
        use alloc::alloc::{alloc_zeroed, Layout};
        use x86_64::VirtAddr;
        
        assert!(memory_size.is_power_of_two(), "Memory size must be a power of two");
        assert!(state_size.is_power_of_two(), "State size must be a power of two");
        
        // MPK requires page-level granularity, so we must page-align our allocations
        let mem_layout = Layout::from_size_align(memory_size.max(4096), 4096).unwrap();
        let state_layout = Layout::from_size_align((state_size * 8).max(4096), 4096).unwrap();
        
        let memory_ptr = unsafe { alloc_zeroed(mem_layout) };
        let state_ptr = unsafe { alloc_zeroed(state_layout) as *mut u64 };

        // Tag the pages with the MPK key
        crate::mpk::tag_page(VirtAddr::new(memory_ptr as u64), mpk_key, hhdm_offset);
        crate::mpk::tag_page(VirtAddr::new(state_ptr as u64), mpk_key, hhdm_offset);

        Self {
            memory_ptr,
            memory_size,
            state_ptr,
            state_size,
            jit,
            vm: AshVm::new(),
            mpk_key,
        }
    }

    /// µFork (Micro-fork): 
    /// Instantly duplicates the sandbox process. 
    /// - JIT Code is shared automatically (Arc/RX Page).
    /// - Data and State are deep-copied via memcpy (faster than CR3 CoW faults for small sandboxes).
    pub fn ufork(&self, new_mpk_key: u8, hhdm_offset: u64) -> Self {
        use alloc::alloc::{alloc_zeroed, Layout};
        use x86_64::VirtAddr;

        let mem_layout = Layout::from_size_align(self.memory_size.max(4096), 4096).unwrap();
        let state_layout = Layout::from_size_align((self.state_size * 8).max(4096), 4096).unwrap();
        
        let memory_ptr = unsafe { alloc_zeroed(mem_layout) };
        let state_ptr = unsafe { alloc_zeroed(state_layout) as *mut u64 };
        
        unsafe {
            core::ptr::copy_nonoverlapping(self.memory_ptr, memory_ptr, self.memory_size);
            core::ptr::copy_nonoverlapping(self.state_ptr, state_ptr, self.state_size);
        }

        crate::mpk::tag_page(VirtAddr::new(memory_ptr as u64), new_mpk_key, hhdm_offset);
        crate::mpk::tag_page(VirtAddr::new(state_ptr as u64), new_mpk_key, hhdm_offset);

        Self {
            memory_ptr,
            memory_size: self.memory_size,
            state_ptr,
            state_size: self.state_size,
            jit: Arc::clone(&self.jit),
            vm: self.vm.clone(),
            mpk_key: new_mpk_key,
        }
    }

    /// Temporarily unlocks this SIP's memory for Kernel access (Ring 0)
    pub fn allow_access(&self) {
        let mut pkrs = 0xFFFFFFFF; // Lock all 16 keys
        pkrs = crate::mpk::set_key_rights(pkrs, 0, false, false); // Unlock Key 0 (Kernel)
        pkrs = crate::mpk::set_key_rights(pkrs, self.mpk_key, false, false); // Unlock this SIP
        crate::mpk::write_pkrs(pkrs);
    }

    /// Locks this SIP's memory, preventing even the Kernel from accessing it
    pub fn revoke_access(&self) {
        let mut pkrs = 0xFFFFFFFF; // Lock all 16 keys
        pkrs = crate::mpk::set_key_rights(pkrs, 0, false, false); // Unlock Key 0 (Kernel)
        crate::mpk::write_pkrs(pkrs);
    }

    pub fn memory_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.memory_ptr, self.memory_size) }
    }

    pub fn state(&self) -> &[u64] {
        unsafe { core::slice::from_raw_parts(self.state_ptr, self.state_size) }
    }

    pub fn execute(&mut self) -> u64 {
        let mut ctx = unsafe {
            AshContext {
                memory: CheriCap::new_root(self.memory_ptr, self.memory_size, Perms::RW),
                state: CheriCap::new_root(self.state_ptr, self.state_size, Perms::RW),
            }
        };
        
        // --- PKS Key Multiplexing (動的キー再割り当て機構) ---
        // ハードウェア制限(15個)を突破するため、現在の実行コア固有のキーを動的に割り当てる
        let active_key = (crate::apic::lapic_id() % 14) as u8 + 1; // Key 1~14
        self.mpk_key = active_key; // 現在のキー状態を更新

        let hhdm_offset = crate::HHDM_REQUEST.response().unwrap().offset;
        
        // PTE上のPKSキーを現在のコア用に書き換える
        crate::mpk::tag_page(x86_64::VirtAddr::new(self.memory_ptr as u64), active_key, hhdm_offset);
        crate::mpk::tag_page(x86_64::VirtAddr::new(self.state_ptr as u64), active_key, hhdm_offset);

        // キャッシュ（TLB）をローカルフラッシュし、新しいキーをプロセッサに認識させる
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(self.memory_ptr as u64));
        x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(self.state_ptr as u64));

        // MPK Isolation: このコアの割り当てキー(active_key)のみをアンロックする
        let mut pkrs = 0xFFFFFFFF; // Lock all 16 keys (AD=1, WD=1)
        pkrs = crate::mpk::set_key_rights(pkrs, 0, false, false); // Unlock Key 0 (Kernel)
        pkrs = crate::mpk::set_key_rights(pkrs, active_key, false, false); // Unlock this SIP's Active Key
        
        crate::mpk::write_pkrs(pkrs);

        // Intel CAT Isolation: このコア用のL3キャッシュ区画をSIPに割り当て（サイドチャネル攻撃の物理的遮断）
        crate::cat::assign_sip_cache_partition(crate::apic::lapic_id());
        
        let result = unsafe { self.jit.execute(&mut ctx) };
        
        // Re-lock all keys except Key 0
        let mut pkrs_restore = 0xFFFFFFFF;
        pkrs_restore = crate::mpk::set_key_rights(pkrs_restore, 0, false, false);
        crate::mpk::write_pkrs(pkrs_restore);
        
        result
    }
}

impl Drop for AshProcess {
    fn drop(&mut self) {
        use alloc::alloc::{dealloc, Layout};
        let mem_layout = Layout::from_size_align(self.memory_size.max(4096), 4096).unwrap();
        let state_layout = Layout::from_size_align((self.state_size * 8).max(4096), 4096).unwrap();
        unsafe {
            dealloc(self.memory_ptr, mem_layout);
            dealloc(self.state_ptr as *mut u8, state_layout);
        }
    }
}
