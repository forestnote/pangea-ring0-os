use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask};
use x86_64::VirtAddr;

extern "C" {
    fn syscall_handler();
}

pub fn init() {
    unsafe {
        // 1. システムコール命令 (syscall/sysret) の有効化
        Efer::update(|flags| flags.insert(EferFlags::SYSTEM_CALL_EXTENSIONS));

        // 2. syscall 発行時の飛び先アドレス (LSTAR) を設定
        LStar::write(VirtAddr::new(syscall_handler as *const () as usize as u64));

        // 3. syscall 時にクリアするフラグメントマスク (割り込み無効化 IF=0)
        SFMask::write(x86_64::registers::rflags::RFlags::INTERRUPT_FLAG);

        // 4. STAR レジスタの設定
        // Ring 0 でシステムコールを受けるため、Kernel CS/SS のセグメントを指定する。
        // GDTのインデックス1が Kernel CS (0x8), 2が Kernel SS (0x10)。
        let mut star: u64 = 0;
        star |= (0x8u64) << 32; // Syscall CS = 0x8 (SS will be Syscall CS + 8 = 0x10)
        core::arch::asm!("wrmsr", in("ecx") 0xC000_0081u32, in("eax") star as u32, in("edx") (star >> 32) as u32);
    }
}

// ----------------------------------------------------------------------------
// レガシーバイナリ（Ring 0 SIPコンパートメント内で実行されるCプログラム等）が
// 発行する `syscall` を直接受け止める Ring 0 ネイティブハンドラ。
// 特権リングの遷移 (Ring 3 -> 0) が発生しないため、超低レイテンシで処理される。
// ----------------------------------------------------------------------------
core::arch::global_asm!(r#"
.global syscall_handler
syscall_handler:
    // syscall 命令の仕様:
    // 次の命令のアドレス(RIP)が RCX に退避される
    // RFLAGS が R11 に退避される
    // スタック(RSP)は一切変更されない（Ring 0 から Ring 0 の呼び出しのため極めて安全）

    // 戻り先情報の退避
    push rcx
    push r11
    
    // SyscallFrame の構築 (システムコールの引数退避)
    push rax // Syscall No
    push rdi // Arg 1
    push rsi // Arg 2
    push rdx // Arg 3
    push r10 // Arg 4
    push r8  // Arg 5
    push r9  // Arg 6
    
    // 第1引数 (RDI) に SyscallFrame のポインタを渡す
    mov rdi, rsp
    
    // Rust のルーターへ処理を委譲
    call rust_syscall_router
    
    // 戻り値を RAX の位置 (スタックの一番上) に上書き
    mov [rsp + 48], rax
    
    // フレームの破棄とレジスタ復元
    pop r9
    pop r8
    pop r10
    pop rdx
    pop rsi
    pop rdi
    pop rax
    
    // 戻り先情報の復元
    pop r11
    pop rcx
    
    // RFLAGS の復元
    push r11
    popfq
    
    // Ring 0 のまま呼び出し元へ復帰 (sysretはRing 3へ降格するため使わず、直接ジャンプする)
    jmp rcx
"#);

#[repr(C)]
pub struct SyscallFrame {
    pub r9: u64,
    pub r8: u64,
    pub r10: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rax: u64,
}

#[no_mangle]
pub extern "C" fn rust_syscall_router(frame: &mut SyscallFrame) -> u64 {
    let sys_no = frame.rax;
    
    // Linux互換の POSIX システムコールテーブルに基づくエミュレーション
    match sys_no {
        1 => {
            // sys_write(fd, buf, count)
            let fd = frame.rdi;
            let buf_ptr = frame.rsi as *const u8;
            let count = frame.rdx as usize;
            
            if fd == 1 || fd == 2 {
                // 標準出力 / 標準エラー出力をカーネルのシリアルコンソールにリダイレクト
                unsafe {
                    let s = core::slice::from_raw_parts(buf_ptr, count);
                    if let Ok(str) = core::str::from_utf8(s) {
                        crate::serial_print!("{}", str);
                    } else {
                        crate::serial_print!("<Invalid UTF-8 string>");
                    }
                }
                return count as u64;
            }
            0
        }
        60 => {
            // sys_exit(error_code)
            let code = frame.rdi;
            crate::serial_println!("\n[ POSIX ] Process executed sys_exit with code: {}", code);
            // 実際のエグゼキュータではここでタスクを終了させる。
            // 現在は単に0を返す。
            0
        }
        _ => {
            crate::serial_println!("\n[ POSIX ] Unhandled Syscall Number: {}", sys_no);
            // -ENOSYS (Linux ABI)
            (!0u64).wrapping_add(38 + 1)
        }
    }
}
