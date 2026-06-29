use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use spin::Mutex;

/// 送信側と受信側で共有される内部状態
struct Shared<T> {
    queue: VecDeque<T>,
    capacity: usize,
    rx_waker: Option<Waker>,
    tx_wakers: alloc::vec::Vec<Waker>,
}

/// 【送信エンドポイント】
/// 意図的に `Clone` を実装し、複数のプロセスから単一のプロセスへ
/// データを送信できる MPSC (Multi-Producer, Single-Consumer) 構成を許容する。
pub struct Sender<T> {
    shared: Arc<Mutex<Shared<T>>>,
}

impl<T> Sender<T> {
    /// 非同期にデータを送信する Future を返す。
    /// キューが満杯の場合は、空き容量ができるまで現在のタスクを休止（Yield）させる（バックプレッシャーの実現）。
    pub fn send(&self, data: T) -> SendFuture<T> {
        SendFuture {
            shared: Arc::clone(&self.shared),
            data: Some(data),
        }
    }
}

/// 送信用の非同期ステートマシン (Future)
pub struct SendFuture<T> {
    shared: Arc<Mutex<Shared<T>>>,
    data: Option<T>,
}

impl<T> Future for SendFuture<T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut shared = this.shared.lock();

        // キューに空き容量があるか確認
        if shared.queue.len() < shared.capacity {
            let data = this.data.take().expect("SendFuture polled after completion");
            shared.queue.push_back(data);

            // 受信側が眠っていれば起こす
            if let Some(waker) = shared.rx_waker.take() {
                waker.wake();
            }
            Poll::Ready(())
        } else {
            // 容量がいっぱいの場合はWakerを登録してサスペンド（Backpressure）
            shared.tx_wakers.push(cx.waker().clone());
            Poll::Pending
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
            // 空き容量ができたので、待機している送信者たちを起こす
            for waker in shared.tx_wakers.drain(..) {
                waker.wake();
            }
            Poll::Ready(data)
        } else {
            // データがなければ、現在のタスク（SIP）のWakerを登録して休止（Yield）する
            shared.rx_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// 容量制限付きの特権空間用ゼロコピー非同期チャネルを生成する
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Mutex::new(Shared {
        queue: VecDeque::with_capacity(capacity),
        capacity,
        rx_waker: None,
        tx_wakers: alloc::vec::Vec::new(),
    }));

    (
        Sender { shared: Arc::clone(&shared) },
     Receiver { shared },
    )
}
