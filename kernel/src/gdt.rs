use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use lazy_static::lazy_static;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        // ★ 絶対防壁: ダブルフォルト発生時に確実に逃げ込める「無傷の専用スタック」
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5; // 20KB
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            // 生ポインタの生成自体に unsafe は不要 (Rustの所有権/借用ルールの静的解析網を通過する)
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
            let stack_end = stack_start + STACK_SIZE;

            // x86のスタックは高位から低位へ伸びるため、終端(最上部)のアドレスを返す
            stack_end
        };
        tss
    };
}

lazy_static! {
    pub static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        // Ring 0 (Kernel) のコードセグメントのみを定義
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));

        // Ring 3用のセレクタは完全に破棄

        (gdt, Selectors { code_selector, tss_selector })
    };
}

pub struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::instructions::segmentation::{CS, SS, Segment};
    use x86_64::structures::gdt::SegmentSelector;

    GDT.0.load();
    unsafe {
        // 現在のコードセグメントレジスタ(CS)をRing 0用に書き換え
        CS::set_reg(GDT.1.code_selector);
        // SS (Stack Segment) レジスタに残っている古い(Limineの)セレクタを無効化 (64-bitでは0で正常)
        SS::set_reg(SegmentSelector::new(0, x86_64::PrivilegeLevel::Ring0));
        // CPUにTSSの場所を教え、ISTを有効化する
        load_tss(GDT.1.tss_selector);
    }
    crate::serial_println!("[ OK ] Ring 0 Exclusive GDT & TSS Loaded. IST Active.");
}
