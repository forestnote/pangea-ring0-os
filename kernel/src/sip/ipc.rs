use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use spin::Mutex;

/// 送信側と受信側で共有される内部状態
struct Shared<T> {
    queue: VecDeque<T>,
    waker: Option<Waker>,
}

/// 【送信エンドポイント】
/// 意図的に `Clone` を実装し、複数のプロセスから単一のプロセスへ
/// データを送信できる MPSC (Multi-Producer, Single-Consumer) 構成を許容する。
pub struct Sender<T> {
    shared: Arc<Mutex<Shared<T>>>,
}

impl<T> Sender<T> {
    /// データを送信する。
    /// 引数 `data` の所有権を完全に奪うため、送信元はこれ以降 `data` に一切アクセスできなくなる。
    /// （これが Rust によるコンパイル時のゼロコスト・メモリ保護防壁である）
    pub fn send(&self, data: T) {
        let mut shared = self.shared.lock();
        shared.queue.push_back(data);

        // データがキューに入ったため、受信側プロセスが眠っていれば叩き起こす
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            shared: Arc::clone(&self.shared),
        }
    }
}

/// 【受信エンドポイント】
/// データを受信する権利。これは複製不可能（!Clone）であり、特定の1つのSIPのみが所有できる。
pub struct Receiver<T> {
    shared: Arc<Mutex<Shared<T>>>,
}

impl<T> Receiver<T> {
    /// 非同期にデータを受信するFutureを返す
    pub fn recv(&self) -> RecvFuture<T> {
        RecvFuture {
            shared: Arc::clone(&self.shared),
        }
    }
}

/// 受信用の非同期ステートマシン (Future)
pub struct RecvFuture<T> {
    shared: Arc<Mutex<Shared<T>>>,
}

impl<T> Future for RecvFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut shared = self.shared.lock();

        // キューにデータがあれば、即座に所有権を引き渡す（Zero-Copy）
        if let Some(data) = shared.queue.pop_front() {
            Poll::Ready(data)
        } else {
            // データがなければ、現在のタスク（SIP）のWakerを登録して休止（Yield）する
            shared.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// 特権空間用のゼロコピー非同期チャネルを生成する
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Mutex::new(Shared {
        queue: VecDeque::new(),
                                     waker: None,
    }));

    (
        Sender { shared: Arc::clone(&shared) },
     Receiver { shared },
    )
}
