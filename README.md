<div align="center">
  <h1>PangeaOS Ring 0: The Singularity Engine</h1>
  <p><strong>Version v0.0.1-5 "Software-Isolated Barrier & Zero-Copy IPC"</strong></p>
  <p>
    Rust言語の持つ「強力な型システム」「所有権構造」「ゼロコスト抽象化」を極限まで活かし、ハードウェアの直上（リング0）で動作する次世代のベアメタル・オペレーティングシステムを構築する研究開発プロジェクト。
  </p>
</div>

<hr/>

## 🌌 PangeaOS とは (Vision)
OSの歴史的アーキテクチャ（Multics, UNIX, Plan 9, L4マイクロカーネル）をリスペクトしつつ、現代のサイバーセキュリティにおける「攻撃者の視点」を先回りした防御的設計（Offensive Defense）を組み込んだ次世代OSです。
既存のOSが抱える「マザーボードやファームウェア（UEFI/ACPI）の挙動に対する盲信」を完全に破壊し、純粋なソフトウェアによるハードウェアの絶対的支配を確立することを目指します。

v0.0.1-5 では、これまでの強固なシングルコア・マルチコア基盤の上に、本プロジェクトの真の特異点である**「ソフトウェア分離プロセス (SIPs)」**と**「所有権駆動型ゼロコピーIPC」**が実装されました。ハードウェアの隔離機能（MMU）を完全に捨て去り、コンパイラレベルでメモリの安全性を証明する、かつてない超高速・高セキュアな特権空間が完成しています。

---

## 🎯 究極のハッキング・アーキテクチャ (Core Architecture)

一般的なOS開発のセオリーを意図的に逸脱した、以下の独自アーキテクチャを採用しています。

### 1. Software-Isolated Processes (SIPs) 【v0.0.1-5 目玉機能 ✨】
Microsoft Researchの「Singularity」OSの概念を現代のRustで昇華させました。
ハードウェアの特権レベル（Ring 3）や仮想メモリの分離機能（MMU）に一切頼らず、Rustの強力な「所有権システム」と「型システム」のみを保護境界として利用します。これにより、カーネル空間（Ring 0）という単一の特権アドレス空間に複数の独立したプロセスを同居させながら、**他プロセスによるメモリ破壊をコンパイル時に数学的に保証・防止**します。
ページテーブルの切り替えやTLBフラッシュといったコンテキストスイッチのオーバーヘッドは「完全にゼロ」になりました。

### 2. Ownership-Driven Zero-Copy IPC 【v0.0.1-5 目玉機能 ✨】
共有メモリによるデータレースの火種を設計レベルで根絶した「契約ベースのチャネル」です。
プロセス間の通信において、データのコピー（`memcpy`）やシリアライズは一切発生しません。送信側プロセスがペイロード（例: ヒープに確保された数MBの `String` や `Vec`）をチャネルに `send` した瞬間、Rustコンパイラがそのポインタの「所有権」を受信側に物理的に移譲（Move）します。
送信側は以降そのデータに触れることができず（コンパイルエラーとして弾かれます）、OSの特権レベル切り替えなしで光速のプロセス間通信を実現しています。

### 3. Hybrid Async-Interrupt Engine
レガシーな 8259 PIC を完全に物理的破棄（`0xFF` マスクによる沈黙化）し、現代的な Local APIC (Advanced Programmable Interrupt Controller) へと移行。
Rustの `Future` および `Waker` アーキテクチャを Ring 0 空間に持ち込み、ハードウェア割り込みをトリガーとして動作する **「完全協調型 Async-Await タスクスケジューラ」** を完成させました。CPUはI/O待ちの時間を完全に別タスクの実行に回すことが可能になっています。

### 4. True Mesh Allocator (Size-Class メモリ再利用)
旧式の Bump Allocator を破棄し、外部依存（Crate）ゼロでフルスクラッチ開発した **Size-Class ベースの Segregated Free List アロケータ** を実装しました。
8, 16, 32... から 2048 バイトまでのオブジェクトサイズに応じてメモリを動的に切り出し、解放時（`drop`）に即座にフリーリストに還元します。これにより、Ring 0 空間で `Box`, `Vec`, `String` といったRustの強力な動的データ構造を、メモリリークの恐怖なしに無制限に利用することが可能です。

### 5. Ring 0 Exclusive GDT & True IDT Barrier (絶対防壁)
ユーザー空間への依存を完全に捨て去り、Ring 0専占のGDT（Global Descriptor Table）とTSS（Task State Segment）を構築。IST（Interrupt Stack Table）を有効化することで、カーネルが致命的なダブルフォルトを引き起こした場合でも、無傷の専用スタックへ確実に逃げ込み、システムクラッシュ（トリプルフォルト）を物理的に阻止します。

### 6. Physical Memory Mastery & Virtual Folding
*   **PMM:** UEFI/Limineブートローダが独占していたメモリマップを強奪。全RAM領域を4KBのページフレームに分割し、128KBの極小Bitmap配列によって数GBの物理メモリをO(1)で管理・割り当て可能なシステムを実装しました。
*   **VMM:** CPUの `CR3` レジスタからアクティブなPML4テーブルを奪取。Rustの所有権システムと統合されたページマッパーを構築し、仮想空間と物理空間のマッピングを完全に支配しています。

### 7. Zero-Stack Double Buffering
描画エンジンはスタックメモリを一切消費しません。すべてのバッファ（最大 2560x1600 解像度に対応する約16MBの裏画面）は、カーネルの静的メモリ領域（`.bss`）に直接マッピングされています。
`slice::fill` を用いた超高速クリアと `core::ptr::copy_nonoverlapping` による物理VRAMへの一撃転送により、画面のチラつき（フリッカー）を完全に消滅させました。

### 8. SMP Ignition & Processor Parking
Limine ブートローダの最新プロトコル (`MpRequest`) を利用してすべてのサブコア (Application Processors) をセーフティに初期化。各コアは起動直後に自身の APIC ID を自己認識し、Ring 0 空間の専用エントリポイント (`ap_main`) にて安全に待機（hltループ）します。

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

    ⚠️ 注意事項 (Troubleshooting)
    Nixは安全のため、Gitの管理下にないファイルをビルドコンテキストから隔離します。新しく作成したファイルが認識されない場合は git add してGitのインデックスに登録してください（コミット・プッシュは不要です）。

操作方法 (Controls)

QEMUウィンドウにフォーカスを合わせた状態で、以下のキーが利用可能です。

    PageUp / ArrowUp : ターミナル履歴バッファを上にスクロール

    PageDown / ArrowDown : ターミナル履歴バッファを下にスクロール

🗺️ ロードマップ (Roadmap & Milestones)

Phase 1〜3: 掌握と防壁の構築 (Completed 🎉)

    [x] PMM/VMM による物理・仮想メモリの支配と Ring 0 絶対防壁の構築

    [x] Ring 0 非同期タスク / Async-Await 基盤の構築

    [x] True Mesh Allocator と Local APIC モダン割り込みエンジンへの移行

Phase 4: Ring 0空間における「ソフトウェア定義」の多層防壁構築 (In Progress 🔥)

    [x] v0.0.1-4: マルチコアプロセッシング (SMP: Symmetric Multiprocessing) の初期化

    [x] v0.0.1-5: ソフトウェア分離プロセス (SIPs) の導入とプロセスの隔離コンテキスト化

    [x] v0.0.1-5: 所有権駆動型のゼロコピーIPC（プロセス間通信）の確立

    [ ] ASH (Application-Specific Safe Handlers) による安全な動的コード実行基盤（エキソカーネル・サンドボックス）の錬成

Phase 5: ハードウェア支援による「物理的隔離」とレガシー互換

    [ ] CHERI (制限境界付きポインタ) アーキテクチャの統合によるハードウェアレベルのコンパートメント化

    [ ] µFork (マイクロフォーク) プロセスメカニズムの実装による POSIX 互換性の安全な移植

Phase 6: VFSの完全廃止と「データ管理の特異点」

    [ ] 単一レベルストア (SLS) の実現と、ファイルシステム概念の破棄（直交的永続性の確立）

📜 ライセンス (License)

This project is licensed under the MIT License - see the LICENSE file for details.

Copyright (c) 2026 pangea-ring0-os developers.
