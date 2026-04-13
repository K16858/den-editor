#![allow(dead_code)]
use portable_pty::{Child, ChildKiller, native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{self, Read, Write};
use std::path::Path;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

impl PtySession {
    pub fn spawn(
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> io::Result<(Self, Box<dyn Read + Send>)> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::other(e.to_string()))?;

        let shell = default_shell();
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

        Ok((Self { master: pair.master, writer, killer }, reader))
    }

    /// 子プロセスを強制終了する。`ClosePseudoConsole` がブロックしないようにするため
    /// `PtySession` を drop する前に必ず呼ぶ。
    pub fn kill(&mut self) {
        let _ = self.killer.kill();
    }

    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)
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

fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}
