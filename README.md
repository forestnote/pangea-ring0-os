<div align="center">
<h1>PangeaOS Ring 0: The Singularity Engine</h1>
  <p><strong>Version v0.0.1-9-1 "Ultimate JIT Security (Zero-Day Patches)"</strong></p>
  <p>
    Rust言語の持つ「強力な型システム」「所有権構造」「ゼロコスト抽象化」を極限まで活かし、ハードウェアの直上（リング0）で動作する次世代のベアメタル・オペレーティングシステムを構築する研究開発プロジェクト。
  </p>
</div> <hr/>

🌌 PangeaOS とは (Vision)

PangeaOSは、OSの歴史的アーキテクチャ（Multics, UNIX, Plan 9, L4マイクロカーネル）を深くリスペクトしつつ、現代のサイバーセキュリティにおける「攻撃者の視点」を先回りした防御的設計（Offensive Defense）を組み込んだ次世代OSです。既存のOSが抱える「マザーボードやファームウェア（UEFI/ACPI）の挙動に対する盲信」を完全に破壊し、純粋なソフトウェアによるハードウェアの絶対的支配を確立することを目指します。

v0.0.1-9-1では、Ring 0 JIT Sandboxのセキュリティを「究極の域」へと押し上げました。攻撃者がJITエンジンの即値ロード命令（`LoadImm`）を悪用してシェルコードをメモリ上に埋め込む **"JIT Spraying" 攻撃を完全に無力化する「Constant Blinding (即値の乱数難読化)」** を実装。さらに、重い処理である Ring 0 FFI (Kernel Callbacks) 呼び出しごとに多大な Gas (1000 Gas) を消費するグローバル・ガスリミットを導入し、ループ処理だけでなくFFIの連続呼び出しによる DoS 攻撃も物理的に不可能にしました。加えて、v0.0.1-9-1の緊急パッチにて、**FFI呼び出し時のGasカウンター（r9）破壊バグ**、および **SIBバイトエンコーディングの欠陥による任意のカーネルメモリ読み書きのゼロデイ脆弱性** を完全に塞ぎ、いかなる未知のパケットペイロードであってもカーネルを破壊・停止させることが不可能な絶対防壁が完成しました。




🎯 究極のハッキング・アーキテクチャ (Core Architecture)

一般的なOS開発のセオリーを意図的に逸脱した、以下の独自アーキテクチャを採用しています。

1. Ring 0 JIT Compiler & Zero-Cost MBC 【v0.0.1-9-1 目玉機能 ✨】

PangeaOSの核となるのは、特権空間（Ring 0）の内部で稼働する超高速な動的ネイティブコード生成エンジンです。このJITコンパイラは、外部の未検証コード（ASHバイトコード）を、実行時にx86_64の純粋なマシン語へ直接トランスパイルします。v0.0.1-9-1では、単一パケットの検査・チューリング完全なループ・状態保持・Ring 0 FFI に加え、**Constant Blinding (JIT Spraying 防御)** と **FFI へのグローバル Gas リミット** を備え、悪意のある入力からカーネルを完全に保護します。

最大の特異点は「境界チェックの完全ブランチレス化 (MBC)」です。領域外メモリアクセスを防ぐため、実行時の分岐（if 文）を挿入するのではなく、トランスパイル段階で物理的なビットマスク（and r8, 0x3F）をスクラッチレジスタを介して強制挿入します。これにより、コンパイル時にオフセットが不明な動的パケットアクセスであっても、OSをクラッシュさせることなく、C言語のハードコードと同等以上の光速実行を維持します。さらに、フォワードジャンプは常に命令境界へ正確に着地するようバイトオフセットを計算するため、Control Flow Integrity (CFI) がハードウェアレベルで保証されています。

JITコンパイラは、専用の4KBページをアロケータから確保し、コンパイルされたマシンコードをそこに配置します。このページはW^X Enforcerによって保護され、実行中に書き換えられることを防ぎます。また、CallExt命令を通じて、_rdtsc()による高精度タイマー取得や、serial_println!によるデバッグ出力といったカーネルヘルパー関数を直接呼び出すことが可能です。

2. W^X (Write XOR Execute) Memory Enforcer 【v0.0.1-7 目玉機能 ✨】

攻撃者の視点（エクスプロイト手法）を根絶するための、究極のページテーブル動的制御機構です。JITがマシン語を生成するバッファ（ヒープ領域）は、生成中は「書き込み可能・実行不可（RW + NX）」として保護されます。コンパイル完了直後、VMMに介入してTLBをフラッシュし、特権レベルで「読み取り専用・実行可能（RX）」へとページ属性をフリップ（Seal）させます。これにより、JITコードの実行中に悪意のあるコードが注入されることを物理的に防ぎます。

また、メモリ解放（Drop）の直前に再び「RW + NX」へと浄化（リストア）することで、再利用時のカーネルパニックやバッファオーバーフローを突いたシェルコード実行をアーキテクチャ・レベルで完全に撲滅しています。このメカニズムは、AshJit::seal()およびDrop for AshJitの実装によって実現されており、x86_64::structures::paging::Mapper::update_flagsを用いてページテーブルエントリのフラグを動的に変更し、TLBをフラッシュすることでCPUに新たなセキュリティ境界を強制認識させます。

3. Software-Isolated Processes (SIPs)

Microsoft Researchの「Singularity」OSの概念を現代のRustで昇華させました。SIPsは、ハードウェアの特権レベル（Ring 3）や仮想メモリの分離機能（MMU）に一切頼らず、Rustの「所有権システム」と「型システム」のみを保護境界として利用します。これにより、カーネル空間（Ring 0）に複数の独立プロセスを同居させながら、他プロセスによるメモリ破壊を数学的に保証・防止します。ページテーブル切り替えのオーバーヘッドは完全にゼロです。

各SIPは一意のSipIdを持ち、SipEnvというケイパビリティ・トークンを通じて外部環境とやり取りします。SipEnvはCloneを実装しないことで、所有権の移動のみを許容し、データレースをコンパイル時に物理的に防ぎます。SIPは非同期タスク（Future）として実装され、カーネルのエグゼキュータによってゼロコストでネイティブ実行されます。これにより、コンテキストスイッチの概念を破棄し、極めて効率的な並行処理を実現します。

4. Ownership-Driven Zero-Copy IPC

共有メモリによるデータレースの火種を根絶した「契約ベースのチャネル」です。プロセス間通信においてデータのコピー（memcpy）は一切発生しません。送信プロセスがペイロードをチャネルに send した瞬間、コンパイラがポインタの「所有権」を受信側に物理的に移譲（Move）し、以後の送信側からのアクセスを遮断します。これにより、データの一貫性と安全性を保証しつつ、最高のパフォーマンスを実現します。kernel/src/sip/ipc.rsで実装されるチャネルは、SenderとReceiverのペアを通じて、安全かつ効率的なメッセージパッシングを提供します。

5. Hybrid Async-Interrupt Engine

レガシーな 8259 PIC を物理的破棄（0xFF マスク）し、Local APIC へ移行しました。Rustの Future および Waker アーキテクチャを Ring 0 空間に持ち込み、ハードウェア割り込みをトリガーとして動作する 「完全協調型 Async-Await タスクスケジューラ」 を完成させました。これにより、割り込み処理と非同期タスクの実行がシームレスに統合され、リアルタイム性と応答性の高いシステムを実現します。kernel/src/scheduler.rsには、スレッド管理とタスクスケジューリングのロジックが実装されており、main.rsではSimpleExecutorとTaskを用いてSIPが非同期に実行される様子が示されています。

6. True Mesh Allocator (Size-Class メモリ再利用)

外部依存ゼロでフルスクラッチ開発した Size-Class ベースの Segregated Free List アロケータです。kernel/src/allocator/mesh.rsに実装されており、8〜2048バイトのオブジェクトサイズに応じてメモリを動的に切り出し、解放時に即座にフリーリストへ還元します。これにより、メモリフラグメンテーションを最小限に抑え、効率的なメモリ再利用を実現します。Ring 0 空間で Box, Vec, String をメモリリークの恐怖なしに無制限に利用可能です。Mesh Allocatorは、BLOCK_SIZES配列で定義されたサイズクラスに基づいてメモリブロックを管理し、必要に応じてBumpAllocatorをフォールバックとして使用します。

7. Ring 0 Exclusive GDT & True IDT Barrier (絶対防壁)

ユーザー空間への依存を完全に捨て去り、Ring 0専占のGDT（Global Descriptor Table）とTSS（Task State Segment）を構築しました。IST（Interrupt Stack Table）を有効化し、致命的なダブルフォルト発生時も無傷の専用スタックへ逃げ込みシステムクラッシュを物理的に阻止します。これにより、OSの安定性と堅牢性を極限まで高めています。kernel/src/gdt.rsとkernel/src/interrupts.rsにこれらのメカニズムが実装されています。

8. Physical Memory Mastery & Virtual Folding

•
PMM (Physical Memory Manager): UEFI/Limineからメモリマップを強奪し、全RAMを4KBのページフレームに分割します。128KBの極小Bitmap配列によって数GBの物理メモリを O(1)O(1)
O(1)
 で管理します。kernel/src/pmm.rsに実装されており、物理メモリの効率的な割り当てと解放を可能にします。

•
VMM (Virtual Memory Manager): CPUの CR3 レジスタからアクティブなPML4テーブルを奪取し、Rustの所有権と統合されたページマッパーを構築することで、仮想空間の完全支配を確立しました。kernel/src/memory.rsに実装されており、OffsetPageTableを用いてページテーブルを操作し、メモリ保護やマッピングを動的に制御します。create_per_core_page_table関数は、各コア専用のページテーブルを作成する機能を提供します。

9. Zero-Stack Double Buffering

描画エンジンはスタックメモリを一切消費せず、すべてカーネルの静的メモリ領域（.bss）に直接マッピングされています。超高速クリアと物理VRAMへの一撃転送によりフリッカーを完全に消滅させました。これにより、グラフィックス処理におけるパフォーマンスを最大化し、スムーズな描画を実現します。

10. SMP Ignition & Processor Parking

Limine の MpRequest を利用してすべての Application Processors (AP) をセーフティに初期化します。各コアは自身の APIC ID を自己認識し、Ring 0 空間の専用エントリポイントにて安全に待機（hltループ）します。これにより、マルチコア環境でのOSの起動と管理を効率的に行い、各コアが独立してタスクを実行できる基盤を提供します。kernel/src/smp.rsにSMPの初期化ロジックが実装されています。




🛠 開発環境 (Setup & Build)

本プロジェクトは、ホストOSのグローバル環境を一切汚染しないよう Nix Flakes によって完全な Determinism（再現性）を保証しています。開発者間のコンパイラバージョンの差異によるトラブルは発生しません。

Prerequisites (前提条件)

ホストマシンに Nix パッケージマネージャ がインストールされており、Flakes 機能が有効になっていること。

ビルドと実行 (Build & Deployment)

Nixの開発シェルに入った状態で make を叩くだけで、環境変数の浄化、Rustカーネルのコンパイル、Limine MBRブートローダの注入、ISOイメージの生成、QEMUエミュレータの起動までが一撃で完遂します。

Bash


# 1. 開発環境に入る
nix develop

# 2. ビルド＆QEMU起動
make




⚠️ 注意事項 (Troubleshooting)
Nixは安全のため、Gitの管理下にないファイルをビルドコンテキストから隔離します。新しく作成したファイルが認識されない場合は git add してGitのインデックスに登録してください（コミット・プッシュは不要です）。

操作方法 (Controls)

QEMUウィンドウにフォーカスを合わせた状態で、以下のキーが利用可能です。

•
PageUp / ArrowUp : ターミナル履歴バッファを上にスクロール

•
PageDown / ArrowDown : ターミナル履歴バッファを下にスクロール




## 🗺️ ロードマップ (Roadmap & Milestones)

### ✅ Phase 1〜3: 掌握と防壁の構築 (Completed)
- [x] **PMM/VMM** による物理・仮想メモリの支配と Ring 0 絶対防壁の構築
- [x] **Ring 0 非同期タスク** / Async-Await 基盤の構築
- [x] **True Mesh Allocator** と Local APIC モダン割り込みエンジンへの移行

### ✅ Phase 4: Ring 0空間における「ソフトウェア定義」の多層防壁構築 (Completed)
- [x] **v0.0.1-4**: マルチコアプロセッシング (SMP: Symmetric Multiprocessing) の初期化
- [x] **v0.0.1-5**: ソフトウェア分離プロセス (SIPs) と所有権駆動型のゼロコピーIPCの確立
- [x] **v0.0.1-6-1**: ASH (Application-Specific Safe Handlers) JIT コンパイラの錬成
- [x] **v0.0.1-6-1**: VMM統合による W^X (Write XOR Execute) メモリ防壁の確立
- [x] **v0.0.1-6-2**: 高度なパケット解析用JIT命令 (Shl/Shr/Jne/Jlt) の拡張
- [x] **v0.0.1-6-3**: 動的パケット書き換え機能と Zero-Cost MBC (ブランチレス境界チェック) の導入
- [x] **v0.0.1-6-4**: 状態保持メモリ (Persistent State) による Stateful Firewall 機能の完成
- [x] **v0.0.1-6-5**: Bounded Loops と Endian-Aware Access によるチューリング完全なパケットインスペクション
- [x] **v0.0.1-7**: Ring 0 FFI (Kernel Callbacks) の実装によるJITからカーネル機能への直接アクセス
- [x] **v0.0.1-8**: Gasリミットと厳密な境界検証を備えた真のSecure JIT Sandboxの完成
- [x] **v0.0.1-9**: Constant Blinding (JIT Spraying防御) と FFI グローバル Gas リミットによる絶対防壁の確立
- [x] **v0.0.1-9-1**: FFI時のGasカウンター破壊バグと、SIBエンコーディング欠陥による任意メモリアクセス脆弱性の完全パッチ

### 🚀 Phase 5: ハードウェア支援による「物理的隔離」とレガシー互換 (Upcoming)
- [ ] **CHERI (制限境界付きポインタ)** アーキテクチャの統合によるハードウェアレベルのコンパートメント化
- [ ] **µFork (マイクロフォーク)** プロセスメカニズムの実装による POSIX 互換性の安全な移植

### 🌌 Phase 6: VFSの完全廃止と「データ管理の特異点」
- [ ] **単一レベルストア (SLS)** の実現と、ファイルシステム概念の破棄（直交的永続性の確立）




📜 ライセンス (License)

This project is licensed under the MIT License - see the LICENSE file for details.

Copyright (c) 2026 pangea-ring0-os developers. https://argleton-ghoti.com/

