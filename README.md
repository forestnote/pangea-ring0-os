<div align="center">
  <h1>PangeaOS Ring 0: The Singularity Engine</h1>
  <p><strong>Version v0.0.1-3 "Async Singularity Awakened"</strong></p>
  <p>
    Rust言語の持つ「強力な型システム」「所有権構造」「ゼロコスト抽象化」を極限まで活かし、ハードウェアの直上（リング0）で動作する次世代のベアメタル・オペレーティングシステムを構築する研究開発プロジェクト。
  </p>
</div>

<hr/>

## 🌌 PangeaOS とは (Vision)
OSの歴史的アーキテクチャ（Multics, UNIX, Plan 9, L4マイクロカーネル）をリスペクトしつつ、現代のサイバーセキュリティにおける「攻撃者の視点」を先回りした防御的設計（Offensive Defense）を組み込んだ次世代OSです。
既存のOSが抱える「マザーボードやファームウェア（UEFI/ACPI）の挙動に対する盲信」を完全に破壊し、純粋なソフトウェアによるハードウェアの絶対的支配を確立することを目指します。

v0.0.1-3 では、**「モダンな Local APIC 割り込み」**、**「Async-Await ベースの非同期タスクスケジューラ」**、そして**「再利用可能な Size-Class メモリ折り畳み基盤（True Mesh Allocator）」**が融合し、OSとしての完全な自律稼働（Singularity）を開始しました。

---

## 🎯 究極のハッキング・アーキテクチャ (Core Architecture)

一般的なOS開発のセオリーを意図的に逸脱した、以下の独自アーキテクチャを採用しています。

### 1. Hybrid Async-Interrupt Engine (Phase 3 完了)
レガシーな 8259 PIC を完全に物理的破棄（`0xFF` マスクによる沈黙化）し、現代的な **Local APIC (Advanced Programmable Interrupt Controller)** へと移行。
Rustの `Future` および `Waker` アーキテクチャを Ring 0 空間に持ち込み、ハードウェア割り込みをトリガーとして動作する **「完全協調型 Async-Await タスクスケジューラ」** を完成させました。CPUはI/O待ちの時間を完全に別タスクの実行に回すことが可能になっています。

### 2. True Mesh Allocator (Size-Class メモリ再利用)
旧式の Bump Allocator を破棄し、外部依存（Crate）ゼロでフルスクラッチ開発した **Size-Class ベースの Segregated Free List アロケータ** を実装しました。
8, 16, 32... から 2048 バイトまでのオブジェクトサイズに応じてメモリを動的に切り出し、解放時（`drop`）に即座にフリーリストに還元します。これにより、Ring 0 空間で `Box`, `Vec`, `String` といったRustの強力な動的データ構造を、メモリリークの恐怖なしに無制限に利用することが可能です。

### 3. Ring 0 Exclusive GDT & True IDT Barrier (絶対防壁)
ユーザー空間への依存を完全に捨て去り、Ring 0専占のGDT（Global Descriptor Table）とTSS（Task State Segment）を構築。IST（Interrupt Stack Table）を有効化することで、カーネルが致命的なダブルフォルトを引き起こした場合でも、無傷の専用スタックへ確実に逃げ込み、システムクラッシュ（トリプルフォルト）を物理的に阻止します。

### 4. Physical Memory Mastery (全物理メモリの掌握)
UEFI/Limineブートローダが独占していたメモリマップを強奪。全RAM領域を4KBのページフレームに分割し、128KBの極小Bitmap配列によって数GBの物理メモリを $O(1)$ で管理・割り当て可能な PMM（Physical Memory Manager）を実装しました。

### 5. Virtual Memory Folding (仮想空間の再構築)
CPUの `CR3` レジスタからアクティブなPML4テーブルを奪取。Rustの所有権システムと統合されたページマッパーを構築し、仮想空間と物理空間のマッピングを完全に支配しています。APICのMMIO（Memory Mapped I/O）領域などは、キャッシュ無効化フラグ（`NO_CACHE`）を伴って動的にマッピングされます。

### 6. Zero-Stack Double Buffering & O(1) Block Transfer
描画エンジンはスタックメモリを一切消費しません。すべてのバッファ（最大 2560x1600 解像度に対応する約16MBの裏画面）は、カーネルの静的メモリ領域（`.bss`）に直接マッピングされています。
`slice::fill` を用いた超高速クリアと `core::ptr::copy_nonoverlapping` による物理VRAMへの一撃転送（Block Transfer）により、画面のチラつき（フリッカー）を完全に消滅させました。

---

## 🛠 開発環境 (Setup & Build)

本プロジェクトは、ホストOSのグローバル環境を一切汚染しないよう **Nix Flakes** によって完全な Determinism（再現性）を保証しています。開発者間のコンパイラバージョンの差異によるトラブルは発生しません。

### Prerequisites (前提条件)
ホストマシンに **Nix パッケージマネージャ** がインストールされており、Flakes 機能が有効になっていること。

### ビルドと実行 (Build & Deployment)

Nixの開発シェルに入った状態で `make` を叩くだけで、環境変数の浄化、Rustカーネルのコンパイル、Limine MBRブートローダの注入、ISOイメージの生成、QEMUエミュレータの起動までが**一撃で完遂**します。

```bash
# 1. 開発環境に入る
nix develop

# 2. ビルド＆QEMU起動
make
```

> **⚠️ 注意事項 (Troubleshooting)**
> Nixは安全のため、Gitの管理下にないファイルをビルドコンテキストから隔離します。新しく作成したファイルが認識されない場合は `git add` してGitのインデックスに登録してください（コミット・プッシュは不要です）。

### 操作方法 (Controls)
QEMUウィンドウにフォーカスを合わせた状態で、以下のキーが利用可能です。
- **PageUp / ArrowUp** : ターミナル履歴バッファを上にスクロール
- **PageDown / ArrowDown** : ターミナル履歴バッファを下にスクロール

---

## 🗺️ ロードマップ (Roadmap & Milestones)

**Phase 1 & 2: 掌握と防壁の構築**
- [x] UEFI VRAM の掌握と Double Buffering の実装
- [x] IDT (Interrupt Descriptor Table) と例外捕捉網の構築
- [x] PMM (Page Frame Allocator) による物理メモリの支配
- [x] 4-Level Paging と GDT/TSS の再構築 (Ring 0 絶対防壁)

**Phase 3: 特異点の覚醒 (v0.0.1-3 にて完了 🎉)**
- [x] **Phase 3-1**: Ring 0 非同期タスク / Async-Await 基盤の構築
- [x] **Phase 3-2**: True Mesh Allocator (Size-Class ベース動的メモリ折り畳みアロケータ)
- [x] **Phase 3-3**: Local APIC Initialization (モダン割り込みエンジンへの完全移行)

**Phase 4: 未踏領域 (Upcoming)**
- [ ] マルチコアプロセッシング (SMP: Symmetric Multiprocessing) の初期化
- [ ] Ring 0 特化の実験的アーキテクチャの探求（Ring 3 の不採用）
- [ ] Ring 0 空間における高度なセキュリティモデルと保護機構の構築
- [ ] VFS (Virtual File System) と初期ストレージドライバの実装

---

## 📜 ライセンス (License)

This project is licensed under the MIT License - see the LICENSE file for details.  
Copyright (c) 2026 pangea-ring0-os developers.
