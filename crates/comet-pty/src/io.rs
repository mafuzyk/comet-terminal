//! I/O assíncrono no PTY master.
//!
//! Fornece leitura e escrita não-bloqueante no file descriptor do PTY master
//! usando tokio. Internamente usa `tokio::fs::File` para integração com o reactor.

use crate::error::{PtyError, PtyResult};
use std::io;
use std::os::fd::{FromRawFd, AsRawFd};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

/// Handle de I/O assíncrono para o PTY master.
///
/// Wrapper thread-safe sobre o file descriptor do PTY master que implementa
/// leitura/escrita assíncrona via tokio.
#[derive(Debug)]
pub struct PtyIo {
    fd: Mutex<tokio::fs::File>,
}

impl PtyIo {
    /// Cria novo handle de I/O a partir de um raw file descriptor (Unix) ou handle (Windows).
    ///
    /// # Safety
    /// O FD deve ser válido e pertencente ao PTY master.
    /// O FD será duplicado para evitar problemas de ownership.
    pub unsafe fn from_raw_fd(fd: std::os::fd::RawFd) -> PtyResult<Self> {
        use std::fs::File;
        use std::os::fd::FromRawFd;
        
        // Duplica o FD para que cada wrapper tenha seu próprio
        let dup_fd = libc::dup(fd);
        if dup_fd < 0 {
            return Err(PtyError::from(std::io::Error::last_os_error()));
        }
        
        let file = File::from_raw_fd(dup_fd);
        let fd = tokio::fs::File::from_std(file);
        Ok(Self {
            fd: Mutex::new(fd),
        })
    }

    /// Cria novo handle de I/O a partir de um `std::fs::File` (PTY master).
    pub fn new(file: std::fs::File) -> PtyResult<Self> {
        let fd = tokio::fs::File::from_std(file);
        Ok(Self {
            fd: Mutex::new(fd),
        })
    }

    /// Escreve dados no PTY.
    ///
    /// Retorna número de bytes escritos. Pode ser menor que `data.len()`
    /// se o buffer do kernel estiver cheio.
    pub async fn write(&self, data: &[u8]) -> PtyResult<usize> {
        let mut fd = self.fd.lock().await;
        fd.write(data).await.map_err(Into::into)
    }

    /// Lê dados disponíveis do PTY.
    ///
    /// Retorna bytes lidos. Retorna vetor vazio se não houver dados
    /// disponíveis no momento (non-blocking).
    pub async fn read(&self) -> PtyResult<Vec<u8>> {
        let mut fd = self.fd.lock().await;
        let mut buf = vec![0u8; 65536]; // 64KB buffer
        match fd.read(&mut buf).await {
            Ok(0) => Err(PtyError::Closed), // EOF
            Ok(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Tenta ler dados sem bloquear.
    pub async fn try_read(&self, buf: &mut [u8]) -> PtyResult<usize> {
        let mut fd = self.fd.lock().await;
        match fd.read(buf).await {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(e.into()),
        }
    }

    /// Fecha o PTY.
    pub async fn close(&self) -> PtyResult<()> {
        let mut fd = self.fd.lock().await;
        fd.shutdown().await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_io_creation() {
        // Teste básico de compilação - criação real precisa de PTY real
    }
}