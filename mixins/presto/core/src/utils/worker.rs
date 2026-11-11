//! A small pool of background threads (`Baritone.getExecutor`), for work that mustn't stall the game
//! thread: rescans for blocks, saving files.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Pool {
    tx: Mutex<mpsc::Sender<Job>>,
}

fn pool() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(2, 4);
        for i in 0..threads {
            let rx = rx.clone();
            let _ = std::thread::Builder::new()
                .name(format!("Presto worker {i}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let guard = rx.lock().unwrap_or_else(|e| e.into_inner());
                            guard.recv()
                        };
                        match job {
                            Ok(job) => {
                                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
                                    log::error!("[Presto] a background job panicked");
                                }
                            }
                            Err(_) => return,
                        }
                    }
                });
        }
        Pool { tx: Mutex::new(tx) }
    })
}

/// Runs `job` on a background thread.
pub fn execute(job: impl FnOnce() + Send + 'static) {
    let tx = pool().tx.lock().unwrap_or_else(|e| e.into_inner());
    let _ = tx.send(Box::new(job));
}

/// A value a background job fills in for the game thread to pick up. Clones share the one slot.
pub struct Slot<T>(Arc<Mutex<Option<T>>>);

impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Slot<T> {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }
    pub fn put(&self, v: T) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(v);
    }
    pub fn take(&self) -> Option<T> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}
