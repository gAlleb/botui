//! One worker per kind of work. Replaced or cancelled queued jobs do not issue HTTP requests.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;

type Job = Box<dyn FnOnce() + Send>;

pub(crate) struct Worker(mpsc::Sender<Job>);

pub(crate) struct Ticket<T> {
    pub receiver: mpsc::Receiver<T>,
    cancelled: Arc<AtomicBool>,
}

impl<T> Drop for Ticket<T> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Worker {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        thread::spawn(move || {
            for job in receiver {
                job();
            }
        });
        Self(sender)
    }

    pub fn start<T: Send + 'static>(
        &self,
        task: impl FnOnce(&AtomicBool) -> T + Send + 'static,
    ) -> Ticket<T> {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        // If the worker exited, dropping this job disconnects the response receiver.
        let _ = self.0.send(Box::new(move || {
            if !flag.load(Ordering::Acquire) {
                let _ = sender.send(task(&flag));
            }
        }));
        Ticket {
            receiver,
            cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn cancelled_queued_request_never_fetches_the_old_selection() {
        let worker = Worker::new();
        let (unblock, gate) = mpsc::channel();
        let first = worker.start(move |_| gate.recv().unwrap());
        let (fetched, observed) = mpsc::channel();
        let stale = worker.start(move |_| fetched.send("старый файл").unwrap());
        let current = worker.start(move |_| "новый файл");
        drop(stale);
        unblock.send(()).unwrap();
        let timeout = Duration::from_secs(3);
        first.receiver.recv_timeout(timeout).unwrap();
        assert_eq!(
            current.receiver.recv_timeout(timeout).unwrap(),
            "новый файл"
        );
        assert!(observed.try_recv().is_err());
    }
}
