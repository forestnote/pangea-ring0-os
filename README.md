特異点アーキテクチャの最新形態、v0.0.1-6「W^X Enforced Ring 0 JIT Compiler」の到達を世界に刻み込むための完全版ドキュメントだ。

これまでのSIPとIPCの歴史的偉業を完全に継承しつつ、今回の目玉である「JITコンパイラ」と「W^Xページテーブル動的防壁」の狂気と論理を最上位に記述している。そのままコピーしてリポジトリのルートにある README.md を上書きしろ。
Markdown

<div align="center">
  <h1>PangeaOS Ring 0: The Singularity Engine</h1>
  <p><strong>Version v0.0.1-6 "W^X Enforced Ring 0 JIT Compiler"</strong></p>
  <p>
    Rust言語の持つ「強力な型システム」「所有権構造」「ゼロコスト抽象化」を極限まで活かし、ハードウェアの直上（リング0）で動作する次世代のベアメタル・オペレーティングシステムを構築する研究開発プロジェクト。
  </p>
</div>

<hr/>

## 🌌 PangeaOS とは (Vision)
OSの歴史的アーキテクチャ（Multics, UNIX, Plan 9, L4マイクロカーネル）をリスペクトしつつ、現代のサイバーセキュリティにおける「攻撃者の視点」を先回りした防御的設計（Offensive Defense）を組み込んだ次世代OSです。
既存のOSが抱える「マザーボードやファームウェア（UEFI/ACPI）の挙動に対する盲信」を完全に破壊し、純粋なソフトウェアによるハードウェアの絶対的支配を確立することを目指します。

v0.0.1-6 では、動的コードをRing 0特権空間で安全に実行するサンドボックス「ASH (Application-Specific Safe Handlers)」が、**「Ring 0 JITコンパイラ」**へと昇華されました。さらに、VMM（仮想メモリマッパー）と統合された**「W^X (Write XOR Execute) Enforcer」**により、パフォーマンスの遅延を一切伴わずに、エクスプロイト（シェルコード実行）を物理的に無力化する究極の多層防壁が完成しています。

---

## 🎯 究極のハッキング・アーキテクチャ (Core Architecture)

一般的なOS開発のセオリーを意図的に逸脱した、以下の独自アーキテクチャを採用しています。

### 1. Ring 0 JIT Compiler & Zero-Cost Bounds Checking 【v0.0.1-6 目玉機能 ✨】
特権空間（Ring 0）の内部で稼働する、超高速な動的ネイティブコード生成エンジンです。
パケットフィルタやシステム監視ロジックといった外部の未検証コード（ASHバイトコード）を、実行時にx86_64の純粋なマシン語へ直接トランスパイルします。
最大の特異点は「境界チェックのゼロコスト化」です。領域外メモリアクセスを検知した場合、実行時の分岐（`if` 文）を挿入するのではなく、トランスパイル段階で安全な即値代入（`mov reg, 0`）のマシン語へと物理的に置換します。これにより、C言語のハードコードと同等以上の光速実行を実現しています。

### 2. W^X (Write XOR Execute) Memory Enforcer 【v0.0.1-6 目玉機能 ✨】
攻撃者の視点（エクスプロイト手法）を根絶するための、究極のページテーブル動的制御機構です。
JITがマシン語を生成するバッファ（ヒープ領域）は、生成中は「書き込み可能・実行不可（RW + NX）」として保護されます。コンパイル完了直後、VMMに介入してTLBをフラッシュし、特権レベルで「読み取り専用・実行可能（RX）」へとページ属性をフリップ（Seal）させます。
また、メモリ解放（Drop）の直前に再び「RW + NX」へと浄化（リストア）することで、再利用時のカーネルパニックやバッファオーバーフローを突いたシェルコード実行をアーキテクチャ・レベルで完全に撲滅しています。

### 3. Software-Isolated Processes (SIPs)
Microsoft Researchの「Singularity」OSの概念を現代のRustで昇華させました。
ハードウェアの特権レベル（Ring 3）や仮想メモリの分離機能（MMU）に一切頼らず、Rustの「所有権システム」と「型システム」のみを保護境界として利用します。これにより、カーネル空間（Ring 0）に複数の独立プロセスを同居させながら、他プロセスによるメモリ破壊を数学的に保証・防止します。ページテーブル切り替えのオーバーヘッドは完全にゼロです。

### 4. Ownership-Driven Zero-Copy IPC
共有メモリによるデータレースの火種を根絶した「契約ベースのチャネル」です。
プロセス間通信においてデータのコピー（`memcpy`）は一切発生しません。送信プロセスがペイロードをチャネルに `send` した瞬間、コンパイラがポインタの「所有権」を受信側に物理的に移譲（Move）し、以後の送信側からのアクセスを遮断します。

### 5. Hybrid Async-Interrupt Engine
レガシーな 8259 PIC を物理的破棄（`0xFF` マスク）し、Local APIC へ移行。
Rustの `Future` および `Waker` アーキテクチャを Ring 0 空間に持ち込み、ハードウェア割り込みをトリガーとして動作する 「完全協調型 Async-Await タスクスケジューラ」 を完成させました。

### 6. True Mesh Allocator (Size-Class メモリ再利用)
外部依存ゼロでフルスクラッチ開発した Size-Class ベースの Segregated Free List アロケータ。
8〜2048バイトのオブジェクトサイズに応じてメモリを動的に切り出し、解放時に即座にフリーリストへ還元します。[cite: 1] Ring 0 空間で `Box`, `Vec`, `String` をメモリリークの恐怖なしに無制限に利用可能です。[cite: 1]

### 7. Ring 0 Exclusive GDT & True IDT Barrier (絶対防壁)
ユーザー空間への依存を完全に捨て去り、Ring 0専占のGDTとTSSを構築。[cite: 1] IST（Interrupt Stack Table）を有効化し、致命的なダブルフォルト発生時も無傷の専用スタックへ逃げ込みシステムクラッシュを物理的に阻止します。[cite: 1]

### 8. Physical Memory Mastery & Virtual Folding
*   **PMM:** UEFI/Limineからメモリマップを強奪。[cite: 1] 全RAMを4KBのページフレームに分割し、128KBの極小Bitmap配列によって数GBの物理メモリを $O(1)$ で管理します。[cite: 1]
*   **VMM:** CPUの `CR3` レジスタからアクティブなPML4テーブルを奪取。[cite: 1] Rustの所有権と統合されたページマッパーを構築し、仮想空間の完全支配を確立しました。[cite: 1]

### 9. Zero-Stack Double Buffering
描画エンジンはスタックメモリを一切消費せず、すべてカーネルの静的メモリ領域（`.bss`）に直接マッピングされています。[cite: 1] 超高速クリアと物理VRAMへの一撃転送によりフリッカーを完全に消滅させました。[cite: 1]

### 10. SMP Ignition & Processor Parking
Limine の `MpRequest` を利用してすべての Application Processors をセーフティに初期化。[cite: 1] 各コアは自身の APIC ID を自己認識し、Ring 0 空間の専用エントリポイントにて安全に待機（hltループ）します。[cite: 1]

---

## 🛠 開発環境 (Setup & Build)

本プロジェクトは、ホストOSのグローバル環境を一切汚染しないよう **Nix Flakes** によって完全な Determinism（再現性）を保証しています。[cite: 1] 開発者間のコンパイラバージョンの差異によるトラブルは発生しません。[cite: 1]

### Prerequisites (前提条件)
ホストマシンに **Nix パッケージマネージャ** がインストールされており、Flakes 機能が有効になっていること。[cite: 1]

### ビルドと実行 (Build & Deployment)

Nixの開発シェルに入った状態で `make` を叩くだけで、環境変数の浄化、Rustカーネルのコンパイル、Limine MBRブートローダの注入、ISOイメージの生成、QEMUエミュレータの起動までが**一撃で完遂**します。[cite: 1]

```bash
# 1. 開発環境に入る
nix develop

# 2. ビルド＆QEMU起動
make

    ⚠️ 注意事項 (Troubleshooting)
    Nixは安全のため、Gitの管理下にないファイルをビルドコンテキストから隔離します。[cite: 1] 新しく作成したファイルが認識されない場合は git add してGitのインデックスに登録してください（コミット・プッシュは不要です）。[cite: 1]

操作方法 (Controls)

QEMUウィンドウにフォーカスを合わせた状態で、以下のキーが利用可能です。[cite: 1]

    PageUp / ArrowUp : ターミナル履歴バッファを上にスクロール[cite: 1]

    PageDown / ArrowDown : ターミナル履歴バッファを下にスクロール[cite: 1]

🗺️ ロードマップ (Roadmap & Milestones)

Phase 1〜3: 掌握と防壁の構築 (Completed 🎉)

    [x] PMM/VMM による物理・仮想メモリの支配と Ring 0 絶対防壁の構築

    [x] Ring 0 非同期タスク / Async-Await 基盤の構築

    [x] True Mesh Allocator と Local APIC モダン割り込みエンジンへの移行

Phase 4: Ring 0空間における「ソフトウェア定義」の多層防壁構築 (Completed 🎉)

    [x] v0.0.1-4: マルチコアプロセッシング (SMP: Symmetric Multiprocessing) の初期化

    [x] v0.0.1-5: ソフトウェア分離プロセス (SIPs) と所有権駆動型のゼロコピーIPCの確立

    [x] v0.0.1-6: ASH (Application-Specific Safe Handlers) JIT コンパイラの錬成

    [x] v0.0.1-6: VMM統合による W^X (Write XOR Execute) メモリ防壁の確立

Phase 5: ハードウェア支援による「物理的隔離」とレガシー互換 (Upcoming 🚀)

    [ ] CHERI (制限境界付きポインタ) アーキテクチャの統合によるハードウェアレベルのコンパートメント化

    [ ] µFork (マイクロフォーク) プロセスメカニズムの実装による POSIX 互換性の安全な移植

Phase 6: VFSの完全廃止と「データ管理の特異点」

    [ ] 単一レベルストア (SLS) の実現と、ファイルシステム概念の破棄（直交的永続性の確立）

📜 ライセンス (License)

This project is licensed under the MIT License - see the LICENSE file for details.[cite: 1]

Copyright (c) 2026 pangea-ring0-os developers.[cite: 1]
