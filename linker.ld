/* kernel/linker.ld */
/* Limineプロトコル向け Higher Half カーネルリンカスクリプト */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

PHDRS
{
    text    PT_LOAD    FLAGS((1 << 0) | (1 << 2)) ; /* Execute + Read */
    rodata  PT_LOAD    FLAGS((1 << 2)) ;            /* Read only */
    data    PT_LOAD    FLAGS((1 << 1) | (1 << 2)) ; /* Write + Read */
}

SECTIONS
{
    /* LimineはカーネルをHigher Halfにロードするため、仮想アドレスのベースを指定 */
    . = 0xffffffff80000000;

    .text : {
        *(.text .text.*)
    } :text

    /* ページ境界(4KB)でアライメントし、セクションの権限を厳格に分離する */
    . = ALIGN(0x1000);

    .rodata : {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(0x1000);

    .data : {
        *(.data .data.*)
    } :data

    .bss : {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /* 不要なセクションを破棄 */
    /DISCARD/ : {
        *(.eh_frame)
        *(.note .note.*)
    }
}
