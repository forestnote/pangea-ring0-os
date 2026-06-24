{
  description = "pangea-ring0-OS: A next-generation bare-metal operating system";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable-small";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # rust-toolchain.toml から決定論的にツールチェーンを構築
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      {
        devShells.default = pkgs.mkShell {
          name = "pangea-ring0-OS-dev";

          buildInputs = with pkgs; [
            rustToolchain
            qemu_kvm
            gdb
            llvmPackages.bintools
            acpica-tools
            dtc
            sdcc
            dosfstools
            mtools
            xorriso
          ] ++ [
            (pkgs.writeShellScriptBin "cargo-qemu" ''
              set -e
              # RUST_TARGET_PATH などのコア設定は .cargo/config.toml に記述することを推奨する
              cargo run --bin pangea-ring0-OS
            '')
          ];

          shellHook = ''
            echo "Initializing PangeaOS Ring-0 Development Environment..."

            # Cargo/Rustupのキャッシュをワークスペース内に隔離し、NixストアのRead-Only制約を回避
            export RUSTUP_HOME="$PWD/.rustup"
            export CARGO_HOME="$PWD/.cargo"
            export PATH="$CARGO_HOME/bin:$PATH"

            # LLDを強制し、ベアメタル向けリンク速度とLTOを最適化
            export RUSTFLAGS="-C link-arg=-fuse-ld=lld -C target-cpu=native"

            echo "QEMU: $(qemu-system-x86_64 --version | head -n 1)"
            echo "GDB:  $(gdb --version | head -n 1)"
            echo "Rust: $(rustc --version)"
          '';
        };

        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
