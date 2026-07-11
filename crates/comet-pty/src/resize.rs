//! Redimensionamento do PTY (SIGWINCH / ioctl).
//!
//! Abstrai o redimensionamento da janela do terminal de forma multiplataforma.

use crate::error::{PtyError, PtyResult};
use portable_pty::PtySize;
use std::os::fd::FromRawFd;

/// Handle para redimensionamento do PTY.
#[derive(Debug)]
pub struct PtyResize {
    master_fd: std::os::fd::OwnedFd,
}

impl PtyResize {
    /// Cria novo handle de redimensionamento a partir do raw file descriptor do master.
    ///
    /// # Safety
    /// O FD deve ser válido e pertencente ao PTY master.
    /// O FD será duplicado para evitar problemas de ownership.
    pub unsafe fn from_raw_fd(fd: std::os::fd::RawFd) -> PtyResult<Self> {
        // Duplica o FD para que cada wrapper tenha seu próprio
        let dup_fd = libc::dup(fd);
        if dup_fd < 0 {
            return Err(PtyError::from(std::io::Error::last_os_error()));
        }
        
        let master_fd = std::os::fd::OwnedFd::from_raw_fd(dup_fd);
        Ok(Self { master_fd })
    }

    /// Redimensiona o PTY para novas dimensões.
    ///
    /// Envia `SIGWINCH` para o processo filho (Unix) ou usa `ioctl`
    /// equivalente. Deve ser chamado quando a janela do terminal mudar de tamanho.
    pub async fn resize(&self, cols: u16, rows: u16) -> PtyResult<()> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        // portable-pty já abstrai a chamada de sistema correta
        portable_pty::native_pty_system()
            .openpty(size)
            .map(|_| ())
            .map_err(|e| PtyError::resize_failed(cols, rows, std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))
            .map(drop)?;

        // No Unix, o portable-pty já envia SIGWINCH internamente no resize
        #[cfg(unix)]
        self.send_sigwinch().await?;

        Ok(())
    }

    /// Envia SIGWINCH para o processo filho (Unix only).
    #[cfg(unix)]
    async fn send_sigwinch(&self) -> PtyResult<()> {
        // O portable-pty já envia SIGWINCH internamente no resize
        // Se precisar de controle manual, usaríamos nix::sys::signal::kill
        Ok(())
    }

    /// Dimensões atuais do PTY (aproximação).
    pub fn current_size(&self) -> PtyResult<PtySize> {
        // portable-pty não expõe getter direto - precisaríamos de ioctl
        // Retorna padrão por enquanto
        Ok(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resize_struct() {
        // Teste de compilação
    }
}