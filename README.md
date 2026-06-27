PangeaOS Core

Version: v0.0.1-2-1 "Memory Defense & Allocator Awakened"

Welcome to PangeaOS！
本プロジェクトは、Rust言語の持つ「強力な型システム」「所有権構造」「ゼロコスト抽象化」を極限まで活かし、ハードウェアの直上（リング0）で動作する次世代のベアメタル・オペレーティングシステムを構築する研究開発プロジェクトである。  

OSのアーキテクチャ（MulticsやUNIX、Plan 9、L4マイクロカーネルなどの歴史的知見）をリスペクトしつつ、現代のサイバーセキュリティにおける「攻撃者の視点」を先回りした防御的設計（Offensive Defense）を組み込んでいる。  

本バージョンは、既存のOSが抱える「マザーボードやファームウェア（UEFI/ACPI）の挙動に対する盲信」を完全に破壊し、純粋なソフトウェアによるハードウェアの絶対的支配を確立した「特異点（Singularity）」ベースラインである。v0.0.1-2-1へのアップデートに伴い、全物理メモリの掌握、最新学術研究に基づく仮想メモリの折り畳み基盤（Mesh Primitive）、および警告ゼロ（Absolute Code Quality）によるRust動的メモリ・エコシステムのRing 0への完全移植を達成した。  
🎯 Core Architecture & Rationale

本カーネルは、一般的なOS開発のセオリーを意図的に逸脱した以下のハッキング・アーキテクチャを採用している。  
1. Ring 0 Exclusive GDT & True IDT Barrier (絶対防壁の構築)

ユーザー空間への依存を完全に捨て去り、Ring 0専占のGDT（Global Descriptor Table）とTSS（Task State Segment）を構築。IST（Interrupt Stack Table）を有効化することで、カーネルが致命的なダブルフォルトを引き起こした場合でも、無傷の専用スタックへ確実に逃げ込みシステムクラッシュ（トリプルフォルト）を物理的に阻止する例外捕捉網を完成させた。
2. Physical Memory Mastery (物理メモリの完全掌握)

UEFI/ブートローダ（Limine）が独占していたメモリマップを強奪。全RAM領域を4KBのページフレームに分割し、128KBの極小Bitmap配列によって数GBの物理メモリをO(1)で管理・割り当て可能なPMM（Physical Memory Manager）を実装した。
3. Virtual Memory Folding / Mesh Primitive (仮想空間の折り畳み基盤)

CPUのCR3レジスタからアクティブなPML4テーブル（ページテーブル）を奪取。最新の学術研究である「Meshアロケータ」の概念を導入し、複数の無関係な仮想ページを同一の物理フレームに強制マッピング（折り畳み）するプリミティブをRing 0でノーコストに実行する基盤を確立した。
4. Ring 0 Global Allocator (alloc の解禁)

Meshアーキテクチャを見据えた「Mesh-Ready Bump Allocator」を実装。Rustのツールチェーン（build-std）を完全に制圧し、ベアメタル空間において Box、Vec、String などの強力な動的データ構造を稼働させることに成功した。
5. Zero-Interrupt Absolute Polling (割り込みの完全破棄)

現代のUEFI環境において、旧式ハードウェア（PS/2コントローラ等）の割り込み線（IRQ）はファームウェアによって暗黙的に切断、あるいはSMM（System Management Mode）へ横取りされる危険性がある。
これをバイパスするため、本コアは IDT（割り込み記述子テーブル）および 8259 PIC を物理的に無効化（cli）している。CPUの特権（Ring 0）を行使し、I/Oポート（0x60, 0x64）を毎秒数百万回の速度で直接ポーリングすることで、ハードウェアの結線状態に依存しないレイテンシ・ゼロの入力掌握を実現した。  
6. Hardware Buffer Drain (完全吸い出し機構)

極小のPS/2ハードウェアバッファ（16バイト）がオーバーフローし、入力がロストする現象を防ぐため、ステータスポート（0x64）のフラグが立つ限りデータを1滴残らず吸い出し続ける while ループによるドレイン機構を実装している。  
7. Zero-Stack Double Buffering (スタック破壊の回避)

ブートローダ（Limine）が提供する初期カーネルスタック（約64KB）の枯渇（Stack Overflow / Triple Fault）を絶対防衛する。
描画エンジンはスタックメモリを一切消費しない。すべてのバッファ（1000行の履歴リングバッファ、および最大2560x1600解像度に対応する約16MBの裏画面）は、カーネルの静的メモリ領域（.bss）に直接マッピングされている。  
8. O(1) Batch Rendering (フリッカーの消滅)

slice::fill を用いた裏画面（バックバッファ）の超高速クリアと、core::ptr::copy_nonoverlapping を用いた物理VRAMへの一撃転送（Block Transfer）を採用。画面のチラつき（Screen Tearing）を物理的に消滅させた。  
9. Serial Backdoor (COM1 観測網)

VRAM描画の不具合やパニック発生時における絶対的な観測経路として、COM1ポート（0x3F8）を利用したホストOSへの直接非同期ストリーム出力を確保している。  
🛠 プロジェクトの特徴 (Foundational Features)

    メモリ安全なリング0（カーネル空間）の実装
    Rustの厳格なコンパイル時チェックにより、従来のC/C++製カーネルで多発していた「バッファオーバーフロー」や「データレース」を未定義動作（UB）になる前に物理的に排除する。  

    Nixによる環境の一元管理（Determinism）
    「開発者によってコンパイラのバージョンが違う」等のOS開発で最も苦痛となるトラブルを、Nix Flakesによって完全に解消している。  

    LLVM/LLDリンカの強制適用
    Rustと親和性の高いLLDリンカを強制適用し、リンク時間の短縮とメモリフットプリントを削ぎ落とすLTO（Link Time Optimization）の基盤を整えている。  

📂 ディレクトリ構造（Directory Layout）
Plaintext

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

現在の Zero-Interrupt Polling アーキテクチャには、明確なトレードオフが存在する[cite: 1]。

    CPU Starvation: メインループ内でポーリングを行うため、BSP（Bootstrap Processor）のコア1つが常に使用率100%で稼働する[cite: 1]。熱暴走を緩和するためにループ内に pause 命令を挿入しているが、電力効率は意図的に度外視されている[cite: 1]。

    これは「確実なベースライン」を確立するための戦略的代償である[cite: 1]。次期フェーズにおける Local APIC の初期化およびプリエンプティブ・スケジューラの導入により、この問題はアーキテクチャレベルで解消される[cite: 1]。

🗺️ Roadmap & Milestones

    [x] Phase 2-1: Page Frame Allocator (物理メモリ管理とBitmapの構築)

    [x] Phase 2-2: 4-Level Paging (仮想メモリの隔離とMeshプリミティブの構築)

    [x] Phase 2-3: Kernel Heap Allocator (alloc クレートの解禁と絶対防壁の完成)

    [ ] Phase 3-1: Hardware Timers & Executor (Ring 0 非同期タスク/Async-Await の構築)

    [ ] Phase 3-2: True Mesh Allocator (Size-Class ベースの動的メモリ折り畳みアロケータの実装)

    [ ] Phase 3-3: Local APIC Initialization (モダン割り込みとSMPへの移行)

📜 ライセンス (License)

This project is licensed under the MIT License - see the LICENSE file for details[cite: 1].
Copyright (c) 2026 pangea-ring0-os developers[cite: 1].
