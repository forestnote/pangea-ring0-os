// kernel/build.rs
use std::env;

fn main() {
    // 再ビルドのトリガー条件を厳密に定義
    println!("cargo:rerun-if-changed=src/multiboot_header.asm");
    println!("cargo:rerun-if-changed=src/boot.asm");
    println!("cargo:rerun-if-changed=linker.ld");

    // アセンブリの静的コンパイルとリンク
    cc::Build::new()
    .file("src/multiboot_header.asm")
    .file("src/boot.asm")
    .compile("boot-asm");

    // 【重要】リンカスクリプトの絶対パスを動的に取得してRustコンパイラに注入
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{}/linker.ld", manifest_dir);
}
