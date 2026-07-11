//! Tipos de erro customizados para a camada PTY.
//!
//! Fornece erros tipados, contextualizados e encadeáveis para todas as
//! operações de PTY. Evita `Box<dyn Error>` na API pública.

use std::io;
use thiserror::Error;

/// Alias conveniente para `Result<T, PtyError>`.
pub type PtyResult<T> = Result<T, PtyError>;

/// Erros que podem ocorrer nas operações de PTY.
#[derive(Debug, Error)]
pub enum PtyError {
    /// Falha ao criar o par PTY (master/slave).
    #[error("Falha ao criar PTY: {message}")]
    CreateFailed {
        message: String,
    },

    /// Falha ao spawnear o processo filho.
    #[error("Falha ao spawnar processo '{command}': {source}")]
    SpawnFailed {
        command: String,
        #[source]
        source: io::Error,
    },

    /// Falha na operação de I/O no PTY master.
    #[error("Erro de I/O no PTY: {source}")]
    Io {
        #[source]
        source: io::Error,
    },

    /// Falha ao redimensionar o PTY.
    #[error("Falha ao redimensionar PTY para {cols}x{rows}: {source}")]
    ResizeFailed {
        cols: u16,
        rows: u16,
        #[source]
        source: io::Error,
    },

    /// Falha ao enviar sinal para o processo.
    #[error("Falha ao enviar sinal para processo {pid}: {source}")]
    SignalFailed {
        pid: u32,
        #[source]
        source: io::Error,
    },

    /// Processo filho já terminou.
    #[error("Processo filho (PID {pid}) já terminou com código {exit_code}")]
    ProcessExited {
        pid: u32,
        exit_code: i32,
    },

    /// PTY fechado inesperadamente.
    #[error("PTY fechado inesperadamente")]
    Closed,

    /// Configuração inválida.
    #[error("Configuração inválida: {message}")]
    InvalidConfig {
        message: String,
    },

    /// Operação não suportada na plataforma atual.
    #[error("Operação não suportada: {operation}")]
    Unsupported {
        operation: String,
    },

    /// Timeout em operação assíncrona.
    #[error("Timeout após {ms}ms: {operation}")]
    Timeout {
        ms: u64,
        operation: String,
    },
}

impl PtyError {
    /// Cria erro de spawn com contexto do comando.
    pub fn spawn_failed(command: impl Into<String>, source: io::Error) -> Self {
        Self::SpawnFailed {
            command: command.into(),
            source,
        }
    }

    /// Cria erro de resize com dimensões.
    pub fn resize_failed(cols: u16, rows: u16, source: io::Error) -> Self {
        Self::ResizeFailed { cols, rows, source }
    }

    /// Cria erro de sinal com PID.
    pub fn signal_failed(pid: u32, source: io::Error) -> Self {
        Self::SignalFailed { pid, source }
    }

    /// Cria erro de configuração inválida.
    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self::InvalidConfig {
            message: message.into(),
        }
    }

    /// Cria erro de operação não suportada.
    pub fn unsupported(operation: impl Into<String>) -> Self {
        Self::Unsupported {
            operation: operation.into(),
        }
    }

    /// Cria erro de timeout.
    pub fn timeout(ms: u64, operation: impl Into<String>) -> Self {
        Self::Timeout {
            ms,
            operation: operation.into(),
        }
    }

    /// Cria erro de criação falhou.
    pub fn create_failed(message: impl Into<String>) -> Self {
        Self::CreateFailed {
            message: message.into(),
        }
    }

    /// Verifica se o erro indica que o processo já terminou.
    pub fn is_process_exited(&self) -> bool {
        matches!(self, Self::ProcessExited { .. })
    }

    /// Verifica se o erro é de I/O temporário (retry pode ajudar).
    pub fn is_temporary_io(&self) -> bool {
        matches!(self, Self::Io { source } if source.kind() == io::ErrorKind::WouldBlock
            || source.kind() == io::ErrorKind::Interrupted
            || source.kind() == io::ErrorKind::TimedOut)
    }

    /// Verifica se o PTY foi fechado.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed) || matches!(self, Self::Io { source } if source.kind() == io::ErrorKind::BrokenPipe)
    }
}

/// Extensões para `io::Error` com contexto de PTY.
pub trait IoErrorExt {
    /// Adiciona contexto de operação de PTY.
    fn pty_context(self, operation: &str) -> PtyError;
}

impl IoErrorExt for io::Error {
    fn pty_context(self, _operation: &str) -> PtyError {
        PtyError::Io { source: self }
    }
}

/// Conversão de `portable_pty::Error` (usa anyhow internamente).
impl From<anyhow::Error> for PtyError {
    fn from(err: anyhow::Error) -> Self {
        Self::CreateFailed {
            message: err.to_string(),
        }
    }
}

/// Conversão de `tokio::io::Error` (que é `std::io::Error`).
impl From<tokio::io::Error> for PtyError {
    fn from(err: tokio::io::Error) -> Self {
        Self::Io { source: err.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_constructors() {
        let err = PtyError::spawn_failed("bash", io::Error::new(io::ErrorKind::NotFound, "no such file"));
        assert!(matches!(err, PtyError::SpawnFailed { .. }));
        assert!(err.to_string().contains("bash"));

        let err = PtyError::resize_failed(80, 24, io::Error::new(io::ErrorKind::InvalidInput, "bad size"));
        assert!(matches!(err, PtyError::ResizeFailed { cols: 80, rows: 24, .. }));

        let err = PtyError::signal_failed(1234, io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert!(matches!(err, PtyError::SignalFailed { pid: 1234, .. }));

        let err = PtyError::create_failed("openpty failed");
        assert!(matches!(err, PtyError::CreateFailed { .. }));
    }

    #[test]
    fn test_error_predicates() {
        let err = PtyError::ProcessExited { pid: 1, exit_code: 0 };
        assert!(err.is_process_exited());
        assert!(!err.is_temporary_io());
        assert!(!err.is_closed());

        let err = PtyError::Io { source: io::Error::new(io::ErrorKind::WouldBlock, "block") };
        assert!(err.is_temporary_io());

        let err = PtyError::Closed;
        assert!(err.is_closed());
    }
}