//! Spawn e gerenciamento do PTY (pseudo-terminal).

use portable_pty::{CommandBuilder, PtyPair, PtySize};
use std::io::{Read, Write};
use thiserror::Error;

/// Configuração para spawn do PTY.
#[derive(Debug, Clone)]
pub struct PtyConfig {
    pub shell: String,
    pub args: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

impl Default for PtyConfig {
    fn default() -> Self {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });
        Self {
            shell,
            args: vec![],
            cwd: None,
            env: std::env::vars().collect(),
            cols: 80,
            rows: 24,
        }
    }
}

/// Erros do PTY.
#[derive(Debug, Error)]
pub enum PtyError {
    #[error("Falha ao criar PTY: {0}")]
    Create(#[from] anyhow::Error),
    #[error("Falha no I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Processo falhou: {0}")]
    Process(String),
}

/// Wrapper do processo PTY com reader/writer.
pub struct PtyProcess {
    pair: PtyPair,
    child: Box<dyn portable_pty::Child + Send>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

impl PtyProcess {
    /// Spawna um novo PTY com shell.
    pub fn spawn(config: PtyConfig) -> Result<Self, PtyError> {
        let mut cmd = CommandBuilder::new(config.shell);
        cmd.args(config.args);
        if let Some(cwd) = config.cwd {
            cmd.cwd(cwd);
        }
        for (k, v) in config.env {
            cmd.env(k, v);
        }

        let pair = portable_pty::native_pty_system().openpty(PtySize {
            rows: config.rows,
            cols: config.cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let child = pair.slave.spawn_command(cmd)?;

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        Ok(Self {
            pair,
            child,
            reader,
            writer,
        })
    }

    /// Redimensiona o PTY.
    pub fn resize(&self, size: PtySize) -> Result<(), PtyError> {
        self.pair.master.resize(size)?;
        Ok(())
    }

    /// Reader para stdout/stderr do shell.
    pub fn reader(&mut self) -> &mut dyn Read {
        &mut self.reader
    }

    /// Writer para stdin do shell.
    pub fn writer(&mut self) -> &mut dyn Write {
        &mut self.writer
    }

    /// Aguarda término do processo. Retorna código de saída (0 = sucesso).
    pub fn wait(&mut self) -> Result<i32, PtyError> {
        let status = self.child.wait()?;
        Ok(status.exit_code() as i32)
    }

    /// Mata o processo.
    pub fn kill(&mut self) -> Result<(), PtyError> {
        self.child.kill().map_err(|e| PtyError::Process(e.to_string()))
    }

    /// Retorna o PtyPair para acesso avançado.
    pub fn pair(&self) -> &PtyPair {
        &self.pair
    }
}