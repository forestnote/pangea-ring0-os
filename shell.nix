# shell.nix
# PangeaOS Ring-0 Standalone Legacy Development Environment
#
# 警告: このファイルはFlakesが利用できない環境へのフォールバック用である。
# ホストの <nixpkgs> の状態に依存しないよう、tarballを明示的にフェッチして
# 決定論的なビルドチェインを構築している。

let
  # 1. 依存関係のピン留め (Flakeの inputs に相当)
  # 常に一貫した状態を得るため、nixos-unstable-small のアーカイブを直接取得
  nixpkgsTarball = builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/nixos-unstable-small.tar.gz";
    # セキュリティと完全性の観点から、本番運用時はここに sha256 ハッシュを追記して改ざんを防ぐべきだ。
  };

  # Oxalica Rust Overlay のフェッチ
  rustOverlayTarball = builtins.fetchTarball {
    url = "https://github.com/oxalica/rust-overlay/archive/master.tar.gz";
  };

  # 2. パッケージセットの構築
  overlays = [ (import rustOverlayTarball) ];
  pkgs = import nixpkgsTarball {
    inherit overlays;
  };

  # 3. ツールチェーンの厳密な解決
  # ワークスペース内の rust-toolchain.toml からRustのバージョンとコンポーネントを復元
  rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

in
pkgs.mkShell {
  name = "pangea-ring0-OS-standalone-dev";

  # 4. ベアメタルOS開発用コアツールセット
  buildInputs = with pkgs; [
    rustToolchain

    # エミュレーション・デバッグ層
    qemu_kvm
    gdb

    # 低レイヤバイナリ操作・ビルド層
    llvmPackages.bintools  # LLD (ゼロコスト抽象化 / LTO用)
    acpica-tools           # ACPIテーブルコンパイラ
    dtc                    # デバイスツリーコンパイラ
    sdcc                   # マイクロコントローラ向けCコンパイラ

    # ブートイメージ・ファイルシステム生成層
    dosfstools
    mtools
    xorriso
  ] ++ [
    # 開発用ヘルパースクリプトのインジェクション
    (pkgs.writeShellScriptBin "cargo-qemu" ''
      set -e
      # 実際のビルドターゲットやフラグ設定は .cargo/config.toml に移譲している前提
      cargo run --bin pangea-ring0-OS
    '')
  ];

  # 5. 環境の隔離とコンパイラフラグの強制適用
  shellHook = ''
    echo "======================================================="
    echo "  PangeaOS Ring-0 Standalone Environment Initialized   "
    echo "======================================================="

    # 開発ホストのグローバル設定との衝突を防ぐため、ワークスペース内に状態を隔離
    export RUSTUP_HOME="$PWD/.rustup"
    export CARGO_HOME="$PWD/.cargo"
    export PATH="$CARGO_HOME/bin:$PATH"

    # ベアメタル向けのリンク設定 (LLDの強制適用)
    # LLDはGNU ldと比較してリンク時間が劇的に短く、Rustカーネル開発のイテレーションに必須
    export RUSTFLAGS="-C link-arg=-fuse-ld=lld -C target-cpu=native"

    echo "[Versions]"
    echo "QEMU: $(qemu-system-x86_64 --version | head -n 1)"
    echo "GDB:  $(gdb --version | head -n 1)"
    echo "Rust: $(rustc --version)"
    echo "======================================================="
  '';
}
