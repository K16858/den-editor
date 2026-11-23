use portable_pty::{
    Child, ChildKiller, CommandBuilder, MasterPty, PtySize, SlavePty, native_pty_system,
};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    child: Box<dyn Child + Send + Sync>,
    _slave: Box<dyn SlavePty + Send>,
}

impl PtySession {
    pub fn spawn(cwd: &Path, cols: u16, rows: u16) -> io::Result<(Self, Box<dyn Read + Send>)> {
        let program = shell();
        Self::spawn_command(&program, &[], Some(cwd), &[], cols, rows)
    }

    pub fn spawn_command(
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env_overrides: &[(String, Option<String>)],
        cols: u16,
        rows: u16,
    ) -> io::Result<(Self, Box<dyn Read + Send>)> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::other(e.to_string()))?;

        let mut builder = CommandBuilder::new(program);
        builder.args(args);
        if let Some(dir) = cwd {
            builder.cwd(normalize_cwd(dir));
        }
        for (key, value) in env_overrides {
            match value {
                Some(val) => builder.env(key, val),
                None => builder.env_remove(key),
            }
        }

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
                child,
                _slave: pair.slave,
            },
            reader,
        ))
    }

    pub fn process_id(&self) -> Option<u32> {
        self.child.process_id()
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
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::other(e.to_string()))
    }
}

pub(crate) fn normalize_cwd(cwd: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = cwd.to_string_lossy();
        if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        if let Some(stripped) = raw.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    cwd.to_path_buf()
}

fn shell() -> String {
    if cfg!(windows) {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::normalize_cwd;
    use std::path::Path;

    #[test]
    fn normalize_cwd_strips_windows_verbatim_prefixes() {
        assert_eq!(
            normalize_cwd(Path::new(r"\\?\C:\workspace\proj")),
            Path::new(r"C:\workspace\proj")
        );
        assert_eq!(
            normalize_cwd(Path::new(r"\\?\UNC\server\share\dir")),
            Path::new(r"\\server\share\dir")
        );
        assert_eq!(
            normalize_cwd(Path::new(r"C:\normal\dir")),
            Path::new(r"C:\normal\dir")
        );
    }
}
