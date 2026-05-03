use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, Sender},
};

type Task = Box<dyn FnOnce() + Send>;

pub struct ThreadPool {
    sender: Sender<Task>,
}

impl ThreadPool {
    pub fn new(threads_number: usize) -> Option<Self> {
        if threads_number == 0 {
            return None;
        }

        let (sender, receiver) = std::sync::mpsc::channel::<Task>();
        let receiver = TaskReceiver::new(receiver);

        for _ in 0..threads_number {
            let receiver = receiver.clone();
            std::thread::spawn(move || {
                while let Some(task) = receiver.recv() {
                    task();
                }
            });
        }

        Some(Self { sender })
    }

    pub fn spawn<F: FnOnce() + Send + 'static>(&self, task: F) {
        let task = Box::new(task);
        self.sender.send(task).unwrap();
    }
}

#[derive(Clone)]
struct TaskReceiver(Arc<Mutex<Receiver<Task>>>);

impl TaskReceiver {
    fn recv(&self) -> Option<Task> {
        self.0.lock().unwrap().recv().ok()
    }

    fn new(receiver: Receiver<Task>) -> Self {
        Self(Arc::new(Mutex::new(receiver)))
    }
}
