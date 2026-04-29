use super::{AdapterConfig, DapEnvelope, DapMessage, decode_envelope, encode_envelope};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum DapEvent {
    Message(DapEnvelope),
    Closed,
    Error(String),
}

#[allow(dead_code)]
pub struct DapSession {
    child: Child,
    writer: DapWriter,
    rx: Receiver<DapEvent>,
    next_seq: u64,
}

enum DapWriter {
    Stdio(ChildStdin),
    Tcp(TcpStream),
}

#[allow(dead_code)]
impl DapSession {
    pub fn start(adapter: &AdapterConfig, working_dir: Option<&Path>) -> io::Result<Self> {
        let (child, writer, mut reader): (Child, DapWriter, Box<dyn Read + Send>) =
            if adapter.dap_adapter_type.eq_ignore_ascii_case("dlv-dap") {
                // Delve DAP is TCP-based. Use --client-addr so the server dials us.
                let listener = TcpListener::bind("127.0.0.1:0")?;
                let addr = listener.local_addr()?;
                listener.set_nonblocking(true)?;

                let mut command = Command::new(&adapter.command);
                command
                    .args(&adapter.args)
                    .arg("--client-addr")
                    .arg(addr.to_string())
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if let Some(dir) = working_dir {
                    command.current_dir(dir);
                }
                let mut child = command.spawn()?;

                let deadline = Instant::now() + Duration::from_secs(3);
                let stream = loop {
                    match listener.accept() {
                        Ok((stream, _peer)) => break stream,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            if Instant::now() >= deadline {
                                let _ = child.kill();
                                return Err(io::Error::new(
                                    io::ErrorKind::TimedOut,
                                    "timed out waiting for dlv dap connection",
                                ));
                            }
                            thread::sleep(Duration::from_millis(20));
                        }
                        Err(e) => {
                            let _ = child.kill();
                            return Err(e);
                        }
                    }
                };
                // After accept, switch to blocking mode for normal framed reads.
                stream.set_nonblocking(false)?;
                let reader_stream = stream.try_clone()?;
                (
                    child,
                    DapWriter::Tcp(stream),
                    Box::new(reader_stream) as Box<dyn Read + Send>,
                )
            } else {
                let mut command = Command::new(&adapter.command);
                command
                    .args(&adapter.args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                if let Some(dir) = working_dir {
                    command.current_dir(dir);
                }
                let mut child = command.spawn()?;
                let stdin = child.stdin.take().ok_or_else(|| io::Error::other("stdin missing"))?;
                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| io::Error::other("stdout missing"))?;
                (
                    child,
                    DapWriter::Stdio(stdin),
                    Box::new(stdout) as Box<dyn Read + Send>,
                )
            };

        let (tx, rx) = mpsc::channel::<DapEvent>();
        thread::spawn(move || {
            let mut buf: Vec<u8> = Vec::new();
            let mut tmp = [0u8; 4096];
            'read: loop {
                match reader.read(&mut tmp) {
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
                                    buf.clear();
                                    break 'read;
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

        Ok(Self {
            child,
            writer,
            rx,
            next_seq: 1,
        })
    }

    pub fn send(&mut self, msg: &DapMessage) -> io::Result<()> {
        let bytes = encode_envelope(msg).map_err(|e| io::Error::other(e.to_string()))?;
        match &mut self.writer {
            DapWriter::Stdio(stdin) => {
                stdin.write_all(&bytes)?;
                stdin.flush()
            }
            DapWriter::Tcp(stream) => {
                stream.write_all(&bytes)?;
                stream.flush()
            }
        }
    }

    pub fn send_request(
        &mut self,
        command: &str,
        arguments: serde_json::Value,
    ) -> io::Result<u64> {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.send(&DapMessage::Request {
            seq,
            command: command.to_string(),
            arguments,
        })?;
        Ok(seq)
    }

    pub fn try_recv(&self) -> Option<DapEvent> {
        self.rx.try_recv().ok()
    }

    pub fn stop(&mut self) {
        let _ = self.child.kill();
    }
}
