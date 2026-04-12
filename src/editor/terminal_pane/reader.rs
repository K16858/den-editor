#![allow(dead_code)]
use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub enum PtyEvent {
    Data(Vec<u8>),
    Closed,
}

pub struct ReaderThread {
    handle: thread::JoinHandle<()>,
}

impl ReaderThread {
    pub fn spawn(mut reader: Box<dyn Read + Send>) -> (Self, Receiver<PtyEvent>) {
        let (tx, rx): (Sender<PtyEvent>, Receiver<PtyEvent>) = mpsc::channel();
        let handle = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(PtyEvent::Closed);
                        break;
                    }
                    Ok(n) => {
                        let _ = tx.send(PtyEvent::Data(buf[..n].to_vec()));
                    }
                }
            }
        });
        (Self { handle }, rx)
    }

    pub fn join(self) {
        let _ = self.handle.join();
    }
}
