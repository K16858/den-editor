#![allow(dead_code)]
use std::io::{self, Read};
use std::path::Path;

// ---- Windows: direct ConPTY via win_pty ----

#[cfg(windows)]
use super::win_pty::WinPty;

#[cfg(windows)]
pub struct PtySession {
    inner: WinPty,
}

#[cfg(windows)]
impl PtySession {
    pub fn spawn(cwd: &Path, cols: u16, rows: u16) -> io::Result<(Self, Box<dyn Read + Send>)> {
        let (pty, reader) = WinPty::spawn(cwd, cols, rows)?;
        Ok((Self { inner: pty }, Box::new(reader)))
    }

    pub fn kill(&mut self) {
        self.inner.kill();
    }

    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.inner.write_all(data)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        self.inner.resize(cols, rows)
    }
}

// ---- Non-Windows: portable-pty ----

#[cfg(not(windows))]
use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize, SlavePty};
#[cfg(not(windows))]
use std::io::Write;

#[cfg(not(windows))]
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    _child: Box<dyn Child + Send + Sync>,
    _slave: Box<dyn SlavePty + Send>,
}

#[cfg(not(windows))]
impl PtySession {
    pub fn spawn(cwd: &Path, cols: u16, rows: u16) -> io::Result<(Self, Box<dyn Read + Send>)> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::other(e.to_string()))?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut builder = CommandBuilder::new(&shell);
        builder.cwd(cwd);

        let child: Box<dyn Child + Send + Sync> = pair
            .slave
            .spawn_command(builder)
            .map_err(|e| io::Error::other(e.to_string()))?;

        let killer = child.clone_killer();
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| io::Error::other(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok((
            Self {
                master: pair.master,
                writer,
                killer,
                _child: child,
                _slave: pair.slave,
            },
            reader,
        ))
    }

    pub fn kill(&mut self) {
        let _ = self.killer.kill();
    }

    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::other(e.to_string()))
    }
}
