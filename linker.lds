/* pangea-ring0-OS/linker.lds */
ENTRY(_start)

SECTIONS {
    /* カーネルは物理メモリ0x100000 (1MB) からロードされる想定（Limineの標準動作）*/
    . = 0xFFFFFFFF80000000; /* リナックスのようなハイアドレスマッピング */
    
    .text : ALIGN(4K) {
        *(.text.entry)       /* 最初の命令 */
        *(.text .text.*)     /* その他のコードセクション */
    }

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
        *(.got*)
    }

    .data : ALIGN(8) {
        *(.data .data.*)
    }

    .bss NOCROSSREFS (OVERLAY) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* ヒープ領域（動的確保が必要な場合用）*/
    .heap : ALIGN(8) {
        __heap_start = .;
        . += 1M; /* 初期ヒープサイズ 1MB */
        __heap_end = .;
    }
}
