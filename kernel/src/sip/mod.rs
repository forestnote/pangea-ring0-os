use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll};

// ==========================================
// IPCモジュールの公開 (Phase 4 - Step 2)
// ==========================================
pub mod ipc;

// ==========================================
// ASHモジュールの公開 (Phase 4 - Step 3)
// ==========================================
pub mod ash;

/// SIPを一意に識別するためのID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SipId(u64);

impl SipId {
    pub fn new() -> Self {
        // ロックフリーで安全に一意のIDを発行する。
        // Ring 0の特権空間において、いかなるロックオーバーヘッドも発生させない。
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        SipId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

// ==========================================
// 1. ケイパビリティ・トークン (The Key)
// ==========================================
/// SIPが外部環境とやり取りするための唯一の権限オブジェクト。
/// 意図的に `Clone` を実装しないことで、所有権の移動（Move）のみを許容し、
/// このトークンを他のプロセスと共有する（データレースを起こす）ことをコンパイル時に物理的に防ぐ。
pub struct SipEnv {
    id: SipId,
    // 今後、ここに「このプロセス専用のIPC通信チャネル」や「許可されたメモリ領域」などの
    // アクセス権限（Capabilities）を持たせる。
}

impl SipEnv {
    pub fn id(&self) -> SipId {
        self.id
    }
}

// ==========================================
// 2. ソフトウェア分離プロセス (Software-Isolated Process)
// ==========================================
/// ハードウェアのリングプロテクション（Ring 3）やMMUに依存しない、純粋なソフトウェアプロセス。
pub struct Sip {
    /// エグゼキュータおよびIPC通信においてプロセスを識別するために使用する
    #[allow(dead_code)] // Dead code警告を抑制
    id: SipId,
    /// プロセスの実行状態（非同期のステートマシン）。
    /// ヒープにピン留め(Pin)することで、メモリ上の位置が固定され、安全な自己参照が可能になる。
    future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
}

impl Sip {
    /// 新しいSIPを生成する。
    /// エントリポイントとなる関数(entry)は、必ず `SipEnv` の所有権を奪うものでなければならない。
    /// これにより、不正な環境からのプロセスの起動を静的に遮断する。
    pub fn spawn<F, Fut>(entry: F) -> Self
    where
    F: FnOnce(SipEnv) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
    {
        let id = SipId::new();
        let env = SipEnv { id };

        // SIPの環境(env)をエントリポイントに渡し、非同期タスク(Future)として実体化する
        let future = Box::pin(entry(env));

        Sip { id, future }
    }
}

// ==========================================
// 3. エグゼキュータへの適合 (Executor Integration)
// ==========================================
/// SIP自身をFutureとして振る舞わせることで、カーネルのエグゼキュータに
/// ゼロコストでネイティブ実行させる。コンテキストスイッチの概念を破棄する。
impl Future for Sip {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // 内部に隠蔽された実際のプロセスロジック(Future)のpollを呼び出す
        self.future.as_mut().poll(cx)
    }
}
