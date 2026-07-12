//! Terminal que combina PTY + parser ANSI + estado do core.

use crate::parser::AnsiParser;
use crate::pty::{PtyConfig, PtyError, PtyProcess};
use comet_core::{Attributes, Color, Cursor, Grid, Terminal as CoreTerminal};
use portable_pty::PtySize;

/// Terminal completo: PTY + parser + grade do core.
pub struct PtyTerminal {
    core: CoreTerminal,
    pty: PtyProcess,
    read_buf: Vec<u8>,
}

impl PtyTerminal {
    /// Cria um novo terminal com PTY spawneado.
    pub fn new(config: PtyConfig) -> Result<Self, PtyError> {
        let pty = PtyProcess::spawn(config)?;
        let size = pty.pair().master.get_size()?;
        let core = CoreTerminal::new(size.cols as usize, size.rows as usize);
        Ok(Self {
            core,
            pty,
            read_buf: vec![0u8; 8192],
        })
    }

    /// Retorna referência ao terminal core (estado da grade, cursor, etc).
    pub fn core(&self) -> &CoreTerminal {
        &self.core
    }

    /// Retorna referência mutável ao terminal core.
    pub fn core_mut(&mut self) -> &mut CoreTerminal {
        &mut self.core
    }

    /// Envia bytes para o stdin do shell (teclas do usuário).
    pub fn write_input(&mut self, data: &[u8]) -> Result<usize, PtyError> {
        let written = self.pty.writer().write(data)?;
        self.pty.writer().flush()?;
        Ok(written)
    }

    /// Lê saída do shell, parseia ANSI e atualiza a grade.
    /// Retorna o número de bytes processados.
    pub fn process_output(&mut self) -> Result<usize, PtyError> {
        let n = self.pty.reader().read(&mut self.read_buf)?;
        if n > 0 {
            let mut parser = AnsiParser::new(&mut self.core);
            parser.feed(&self.read_buf[..n]);
        }
        Ok(n)
    }

    /// Redimensiona o terminal (PTY + core).
    pub fn resize(&mut self, cols: usize, rows: usize) -> Result<(), PtyError> {
        let size = PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        };
        self.pty.resize(size)?;
        self.core.resize(cols, rows);
        Ok(())
    }

    /// Aguarda o processo filho terminar.
    pub fn wait(&mut self) -> Result<i32, PtyError> {
        self.pty.wait()
    }

    /// Mata o processo filho.
    pub fn kill(&mut self) -> Result<(), PtyError> {
        self.pty.kill()
    }

    // ===== Delegação conveniente para o core =====

    pub fn width(&self) -> usize {
        self.core.width()
    }

    pub fn height(&self) -> usize {
        self.core.height()
    }

    pub fn grid(&self) -> &Grid {
        self.core.grid()
    }

    pub fn cursor(&self) -> &Cursor {
        self.core.cursor()
    }

    pub fn cursor_mut(&mut self) -> &mut Cursor {
        self.core.cursor_mut()
    }

    pub fn pen_foreground(&self) -> Color {
        self.core.pen_foreground()
    }

    pub fn pen_background(&self) -> Color {
        self.core.pen_background()
    }

    pub fn pen_attributes(&self) -> Attributes {
        self.core.pen_attributes()
    }

    pub fn set_foreground(&mut self, color: Color) {
        self.core.set_foreground(color);
    }

    pub fn set_background(&mut self, color: Color) {
        self.core.set_background(color);
    }

    pub fn set_attributes(&mut self, attrs: Attributes) {
        self.core.set_attributes(attrs);
    }
}
