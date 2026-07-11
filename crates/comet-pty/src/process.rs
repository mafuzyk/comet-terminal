//! Spawn e gerenciamento do processo filho no PTY.
//!
//! Usa `portable-pty` para abstração multiplataforma (Unix: fork+exec, Windows: ConPTY).

use crate::error::{PtyError, PtyResult};
use portable_pty::{CommandBuilder, PtyPair, PtySize};
use std::path::PathBuf;
use std::os::fd::AsRawFd;

/// Configuração para criação do PTY e spawn do shell.
#[derive(Debug, Clone)]
pub struct PtyConfig {
    /// Comando do shell (ex: "bash", "zsh", "fish", "cmd.exe").
    pub shell: String,
    /// Argumentos para o shell (ex: ["-l"] para login shell).
    pub args: Vec<String>,
    /// Diretório de trabalho inicial.
    pub cwd: Option<PathBuf>,
    /// Variáveis de ambiente adicionais (além do ambiente herdado).
    pub env: Vec<(String, String)>,
    /// Largura inicial em colunas.
    pub cols: u16,
    /// Altura inicial em linhas.
    pub rows: u16,
}

impl Default for PtyConfig {
    fn default() -> Self {
        let shell = if cfg!(windows) {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        };

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

/// Processo PTY com handles para master/slave.
pub struct PtyProcess {
    pair: PtyPair,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    pid: Option<u32>,
}

impl std::fmt::Debug for PtyProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyProcess")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl PtyProcess {
    /// Spawna novo processo no PTY.
    pub fn spawn(config: PtyConfig) -> PtyResult<Self> {
        let pty_system = portable_pty::native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::create_failed(e.to_string()))?;

        let mut cmd = CommandBuilder::new(&config.shell);
        cmd.args(&config.args);

        if let Some(cwd) = config.cwd {
            cmd.cwd(cwd);
        }

        for (k, v) in config.env {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::spawn_failed(&config.shell, std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        let pid = child.process_id();

        Ok(Self {
            pair,
            child,
            pid,
        })
    }

    /// Retorna referência ao par PTY (master + slave).
    pub fn pair(&self) -> &PtyPair {
        &self.pair
    }

    /// Retorna referência mutável ao par PTY.
    pub fn pair_mut(&mut self) -> &mut PtyPair {
        &mut self.pair
    }

    /// Retorna o raw file descriptor do master PTY.
    pub fn master_raw_fd(&self) -> PtyResult<i32> {
        self.pair.master.as_raw_fd()
            .ok_or_else(|| PtyError::invalid_config("PTY master FD not available"))
    }

    /// PID do processo filho.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Verifica se o processo ainda está vivo.
    pub fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    /// Aguarda término do processo.
    pub async fn wait(&mut self) -> PtyResult<i32> {
        let status = self
            .child
            .wait()
            .map_err(|e| PtyError::Io { source: e })?;
        Ok(status.exit_code() as i32)
    }

    /// Encerra o processo (SIGTERM -> SIGKILL).
    pub async fn kill(&mut self) -> PtyResult<()> {
        // Tenta SIGTERM primeiro
        if let Err(e) = self.child.kill() {
            return Err(PtyError::signal_failed(self.pid.unwrap_or(0), e));
        }

        // Aguarda um pouco
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Se ainda vivo, força SIGKILL
        if self.is_alive() {
            let _ = self.child.kill();
        }

        Ok(())
    }

    /// Redimensiona o PTY.
    pub fn resize(&self, size: PtySize) -> PtyResult<()> {
        self.pair
            .master
            .resize(size)
            .map_err(|e| PtyError::create_failed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = PtyConfig::default();
        assert!(!config.shell.is_empty());
        assert_eq!(config.cols, 80);
        assert_eq!(config.rows, 24);
        assert!(!config.env.is_empty());
    }

    #[test]
    fn test_config_builder() {
        let config = PtyConfig {
            shell: "bash".to_string(),
            args: vec!["-l".to_string()],
            cwd: Some("/tmp".into()),
            env: vec![("FOO".to_string(), "bar".to_string())],
            cols: 120,
            rows: 40,
        };
        assert_eq!(config.shell, "bash");
        assert_eq!(config.args, vec!["-l"]);
        assert_eq!(config.cwd, Some("/tmp".into()));
        assert_eq!(config.cols, 120);
        assert_eq!(config.rows, 40);
    }
}