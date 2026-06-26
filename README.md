2つのドキュメントが持つ「プロジェクトの初期思想と環境構築の基盤（v0.0.1）」と、「ハードウェアの限界を突破した最新のアーキテクチャ（v0.0.1-2）」を完全に統合し、一切の矛盾がない単一のマスター・ドキュメントを生成した。

リポジトリのルートにある README.md を以下の内容で完全に上書きしろ。
Markdown

# PangeaOS Core
**Version:** v0.0.1-2 "Zero-Interrupt Polling Engine"

Welcome to **PangeaOS**！ 
本プロジェクトは、Rust言語の持つ「強力な型システム」「所有権構造」「ゼロコスト抽象化」を極限まで活かし、ハードウェアの直上（リング0）で動作する次世代のベアメタル・オペレーティングシステムを構築する研究開発プロジェクトである。

OSのアーキテクチャ（MulticsやUNIX、Plan 9、L4マイクロカーネルなどの歴史的知見）をリスペクトしつつ、現代のサイバーセキュリティにおける「攻撃者の視点」を先回りした防御的設計（Offensive Defense）を組み込んでいる。

本バージョン（v0.0.1-2）は、既存のOSが抱える「マザーボードやファームウェア（UEFI/ACPI）の挙動に対する盲信」を完全に破壊し、純粋なソフトウェアによるハードウェアの絶対的支配を確立した「特異点（Singularity）」ベースラインである。

---

## 🎯 Core Architecture & Rationale

本カーネルは、一般的なOS開発のセオリーを意図的に逸脱した以下のハッキング・アーキテクチャを採用している。

### 1. Zero-Interrupt Absolute Polling (割り込みの完全破棄)
現代のUEFI環境において、旧式ハードウェア（PS/2コントローラ等）の割り込み線（IRQ）はファームウェアによって暗黙的に切断、あるいはSMM（System Management Mode）へ横取りされる危険性がある。
これをバイパスするため、本コアは **IDT（割り込み記述子テーブル）および 8259 PIC を物理的に無効化（`cli`）** している。CPUの特権（Ring 0）を行使し、I/Oポート（`0x60`, `0x64`）を毎秒数百万回の速度で直接ポーリングすることで、ハードウェアの結線状態に依存しないレイテンシ・ゼロの入力掌握を実現した。

### 2. Hardware Buffer Drain (完全吸い出し機構)
極小のPS/2ハードウェアバッファ（16バイト）がオーバーフローし、入力がロストする現象を防ぐため、ステータスポート（`0x64`）のフラグが立つ限りデータを1滴残らず吸い出し続ける `while` ループによるドレイン機構を実装している。

### 3. Zero-Stack Double Buffering (スタック破壊の回避)
ブートローダ（Limine）が提供する初期カーネルスタック（約64KB）の枯渇（Stack Overflow / Triple Fault）を絶対防衛する。
描画エンジンはスタックメモリを一切消費しない。すべてのバッファ（1000行の履歴リングバッファ、および最大2560x1600解像度に対応する約16MBの裏画面）は、カーネルの静的メモリ領域（`.bss`）に直接マッピングされている。

### 4. $O(1)$ Batch Rendering (フリッカーの消滅)
`slice::fill` を用いた裏画面（バックバッファ）の超高速クリアと、`core::ptr::copy_nonoverlapping` を用いた物理VRAMへの一撃転送（Block Transfer）を採用。画面のチラつき（Screen Tearing）を物理的に消滅させた。

### 5. Serial Backdoor (COM1 観測網)
VRAM描画の不具合やパニック発生時における絶対的な観測経路として、COM1ポート（`0x3F8`）を利用したホストOSへの直接非同期ストリーム出力を確保している。

---

## 🛠 プロジェクトの特徴 (Foundational Features)

1. **メモリ安全なリング0（カーネル空間）の実装**
   Rustの厳格なコンパイル時チェックにより、従来のC/C++製カーネルで多発していた「バッファオーバーフロー」や「データレース」を未定義動作（UB）になる前に物理的に排除する。
2. **Nixによる環境の一元管理（Determinism）**
   「開発者によってコンパイラのバージョンが違う」等のOS開発で最も苦痛となるトラブルを、Nix Flakesによって完全に解消している。
3. **LLVM/LLDリンカの強制適用**
   Rustと親和性の高いLLDリンカを強制適用し、リンク時間の短縮とメモリフットプリントを削ぎ落とすLTO（Link Time Optimization）の基盤を整えている。

---

## 📂 ディレクトリ構造（Directory Layout）

```text
pangea-ring0-os/
├── Makefile                <- ビルドからISO生成、QEMU起動までを一撃で完遂する自動化スクリプト
├── .cargo/
│   └── config.toml         <- Rustのビルドターゲットやコンパイラフラグを指定する重要設定
├── src/
│   └── main.rs             <- カーネルのエントリポイント（#![no_std] および #![no_main]）
├── flake.nix               <- モダンなNixシステムのための環境定義ファイル
├── shell.nix               <- 従来の nix-shell やCI環境のためのスタンドアローン環境定義ファイル
├── rust-toolchain.toml     <- 使用するRustコンパイラ（Nightly）の厳密な指定
└── README.md               <- 本ドキュメント

⚙ 開発環境の構築（Setup Environment）

本プロジェクトは、ホストOSのグローバル環境を一切汚染しないように設計されている。
Prerequisites (前提条件)

ホストマシンに Nixパッケージマネージャ がインストールされており、Flakes機能が有効になっていること。
🚀 開発環境への入り方

パターンA：モダンな Nix Flakes を使用する場合（推奨）
Bash

nix develop

パターンB：従来の nix-shell コマンドを使用する場合
Bash

nix-shell

    ⚠️ 超重要トラブルシューティング
    Nixは安全のため、Gitの管理下にないファイルをビルドコンテキストから隔離する。新しく作成したファイル（rust-toolchain.toml など）がNixに認識されない場合は、必ず以下のコマンドを叩いてGitのインデックス（ステージング領域）に登録すること（Pushは不要）。
    Bash

    git init
    git add flake.nix shell.nix rust-toolchain.toml src/main.rs .cargo/config.toml Makefile

🏃‍♂️ ビルドと実行方法（Build & Deployment）

Nixの開発シェルに入った状態で、以下のコマンドを実行する。環境変数の浄化からカーネルのコンパイル、ISO生成、QEMUの起動までが一撃で完遂する。
Bash

make

Controls (操作方法)

QEMUウィンドウにフォーカスを合わせた状態で、以下のキーが利用可能。

    PageUp / ArrowUp : 履歴バッファを上にスクロール

    PageDown / ArrowDown : 履歴バッファを下にスクロール

⚠️ Known Trade-offs (研究者向けノート)

現在の Zero-Interrupt Polling アーキテクチャには、明確なトレードオフが存在する。

    CPU Starvation: メインループ内でポーリングを行うため、BSP（Bootstrap Processor）のコア1つが常に使用率100%で稼働する。熱暴走を緩和するためにループ内に pause 命令を挿入しているが、電力効率は意図的に度外視されている。

    これは「確実なベースライン」を確立するための戦略的代償である。次期フェーズにおける Local APIC の初期化およびプリエンプティブ・スケジューラの導入により、この問題はアーキテクチャレベルで解消される。

🗺️ Next Phase: Roadmap

    [ ] Phase 2-1: Page Frame Allocator (物理メモリ管理とBitmapの構築)

    [ ] Phase 2-2: 4-Level Paging (仮想メモリの隔離)

    [ ] Phase 2-3: Kernel Heap Allocator (alloc クレートの解禁)

    [ ] Phase 2-4: Local APIC Initialization (モダン割り込みへの移行)

📜 ライセンス (License)

This project is licensed under the MIT License - see the LICENSE file for details.
Copyright (c) 2026 pangea-ring0-os developers.
