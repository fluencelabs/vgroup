use std::io::Write;
use std::ops::Div;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use futures_util::future::BoxFuture;
use futures_util::future::FutureExt;
use futures_util::stream::FuturesUnordered;
use tokio::runtime::Runtime;
use tokio::task::yield_now;

use crate::get_tid;

pub struct Threads {
    pub stop: Arc<AtomicBool>,
    pub join: BoxFuture<'static, ()>,
    pub ids: Vec<u64>,
    pub runtime: Runtime,
}

impl Threads {
    pub fn new(n: usize) -> Self {
        use tokio::runtime::Builder;

        // let (sender, receiver) = tokio::sync::mpsc::channel(n);
        let (sender, receiver) = std::sync::mpsc::channel();
        let runtime = Builder::new_multi_thread()
            .worker_threads(n)
            .max_blocking_threads(1)
            .on_thread_start(move || {
                let tid = get_tid();
                println!("thread started: {}", tid);
                sender.send(tid).expect("send");
            })
            .build()
            .expect("build tokio runtime");

        let stop = Arc::new(AtomicBool::new(false));
        let join = (0..n)
            .map(|_| runtime.spawn(thread_body(stop.clone())))
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .map(|_| ())
            .boxed();

        // TODO: it's possible it will hang here without any way to notify user
        let thread_ids = receiver.into_iter().take(n).collect::<Vec<_>>();

        Threads {
            stop,
            join,
            ids: thread_ids,
            runtime,
        }
    }
}

async fn thread_body(stop: Arc<AtomicBool>) {
    yield_now().await;
    loop {
        if stop.load(Ordering::SeqCst) {
            println!("thread stopped: {}", get_tid());
            std::io::stdout().flush().expect("flush");
            break;
        }

        let floats: Vec<f64> = (1..1000000).map(|n| 1f64 / n as f64).collect::<Vec<_>>();
        let sum: f64 = floats.clone().into_iter().sum();
        let floats = floats
            .clone()
            .into_iter()
            .map(|f| {
                let exp = f.exp();
                let exp2 = f.exp2();

                sum.div(exp) - exp2.div(sum)
            })
            .collect::<Vec<_>>();
        let sum: f64 = floats.into_iter().sum::<f64>() + sum;
        if sum < 0f64 {
            println!("DID NOT EXPECT THAT");
        }
    }
}