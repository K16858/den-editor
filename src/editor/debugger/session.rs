use super::{AdapterConfig, DapEnvelope, DapMessage, decode_envelope, encode_envelope};
use std::io::{self, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[derive(Debug)]
pub enum DapEvent {
    Message(DapEnvelope),
    Closed,
    Error(String),
}

#[allow(dead_code)]
pub struct DapSession {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<DapEvent>,
}

#[allow(dead_code)]
impl DapSession {
    pub fn start(adapter: &AdapterConfig) -> io::Result<Self> {
        let mut command = Command::new(&adapter.command);
        command
            .args(&adapter.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| io::Error::other("stdin missing"))?;
        let mut stdout = child.stdout.take().ok_or_else(|| io::Error::other("stdout missing"))?;

        let (tx, rx) = mpsc::channel::<DapEvent>();
        thread::spawn(move || {
            let mut buf: Vec<u8> = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match stdout.read(&mut tmp) {
                    Ok(0) => {
                        let _ = tx.send(DapEvent::Closed);
                        break;
                    }
                    Ok(n) => {
                        buf.extend_from_slice(&tmp[..n]);
                        loop {
                            match decode_envelope(&buf) {
                                Ok(Some((env, consumed))) => {
                                    buf.drain(0..consumed);
                                    let _ = tx.send(DapEvent::Message(env));
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    let _ = tx.send(DapEvent::Error(e.to_string()));
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(DapEvent::Error(e.to_string()));
                        break;
                    }
                }
            }
        });

        Ok(Self { child, stdin, rx })
    }

    pub fn send(&mut self, msg: &DapMessage) -> io::Result<()> {
        let bytes = encode_envelope(msg).map_err(|e| io::Error::other(e.to_string()))?;
        self.stdin.write_all(&bytes)?;
        self.stdin.flush()
    }

    pub fn try_recv(&self) -> Option<DapEvent> {
        self.rx.try_recv().ok()
    }

    pub fn stop(&mut self) {
        let _ = self.child.kill();
    }
}
