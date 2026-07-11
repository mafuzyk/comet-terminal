//! Parser de sequências ANSI usando `vte`.
//!
//! Traduz bytes brutos do PTY em ações sobre [`comet_core::Terminal`].

use comet_core::{Attributes, Cell, Color, Terminal};
use vte::{Parser, Perform, Params};

/// Implementa `vte::Perform` aplicando operações diretamente no `Terminal`.
pub struct AnsiParser<'a> {
    terminal: &'a mut Terminal,
}

impl<'a> AnsiParser<'a> {
    /// Cria um parser ligado a um terminal.
    pub fn new(terminal: &'a mut Terminal) -> Self {
        Self { terminal }
    }

    /// Processa um chunk de bytes do PTY.
    pub fn feed(&mut self, data: &[u8]) {
        let mut parser = Parser::new();
        for &byte in data {
            parser.advance(self, byte);
        }
    }

    /// Obtém parâmetro CSI por índice (0-based). Retorna 0 se não existir.
    fn param_or_zero(&self, params: &Params, i: usize) -> i64 {
        params.iter().nth(i).and_then(|v| v.first().copied()).unwrap_or(0) as i64
    }

    // ===== Helper methods =====

    fn handle_erase_display(&mut self, mode: i64) {
        let (cx, cy) = self.terminal.cursor().position();
        let width = self.terminal.width();
        let height = self.terminal.height();

        match mode {
            0 => { // From cursor to end
                for x in cx..width {
                    self.terminal.grid_mut().set(x, cy, Cell::blank());
                }
                for y in (cy + 1)..height {
                    for x in 0..width {
                        self.terminal.grid_mut().set(x, y, Cell::blank());
                    }
                }
            }
            1 => { // From start to cursor
                for y in 0..=cy {
                    let end = if y == cy { cx + 1 } else { width };
                    for x in 0..end {
                        self.terminal.grid_mut().set(x, y, Cell::blank());
                    }
                }
            }
            2 | 3 => { // Entire screen
                self.terminal.clear();
            }
            _ => {}
        }
    }

    fn handle_erase_line(&mut self, mode: i64) {
        let (cx, cy) = self.terminal.cursor().position();
        let width = self.terminal.width();

        match mode {
            0 => { // From cursor to end
                for x in cx..width {
                    self.terminal.grid_mut().set(x, cy, Cell::blank());
                }
            }
            1 => { // From start to cursor
                for x in 0..=cx {
                    self.terminal.grid_mut().set(x, cy, Cell::blank());
                }
            }
            2 => { // Entire line
                for x in 0..width {
                    self.terminal.grid_mut().set(x, cy, Cell::blank());
                }
            }
            _ => {}
        }
    }

    fn handle_sgr(&mut self, params: &Params) {
        let mut attrs = self.terminal.pen_attributes();
        let mut fg = self.terminal.pen_foreground();
        let mut bg = self.terminal.pen_background();

        let mut i = 0;
        while i < params.len() {
            let param = self.param_or_zero(params, i);

            match param {
                0 => { // Reset
                    attrs = Attributes::default();
                    fg = Color::Default;
                    bg = Color::Default;
                }
                1 => attrs.bold = true,
                2 => attrs.dim = true,
                3 => attrs.italic = true,
                4 => attrs.underline = true,
                7 => attrs.reverse = true,
                9 => attrs.strikethrough = true,
                22 => { attrs.bold = false; attrs.dim = false; }
                23 => attrs.italic = false,
                24 => attrs.underline = false,
                27 => attrs.reverse = false,
                29 => attrs.strikethrough = false,
                30..=37 => fg = ansi_standard_fg(param - 30),
                38 => { // Extended foreground
                    if i + 1 < params.len() {
                        let sub = self.param_or_zero(params, i + 1);
                        if sub == 5 && i + 2 < params.len() {
                            // 38;5;n - 256 color
                            let n = self.param_or_zero(params, i + 2) as u8;
                            fg = Color::Indexed(n);
                            i += 2;
                        } else if sub == 2 && i + 4 < params.len() {
                            // 38;2;r;g;b - truecolor
                            let r = self.param_or_zero(params, i + 2) as u8;
                            let g = self.param_or_zero(params, i + 3) as u8;
                            let b = self.param_or_zero(params, i + 4) as u8;
                            fg = Color::Rgb(r, g, b);
                            i += 4;
                        }
                    }
                }
                39 => fg = Color::Default,
                40..=47 => bg = ansi_standard_bg(param - 40),
                48 => { // Extended background
                    if i + 1 < params.len() {
                        let sub = self.param_or_zero(params, i + 1);
                        if sub == 5 && i + 2 < params.len() {
                            let n = self.param_or_zero(params, i + 2) as u8;
                            bg = Color::Indexed(n);
                            i += 2;
                        } else if sub == 2 && i + 4 < params.len() {
                            let r = self.param_or_zero(params, i + 2) as u8;
                            let g = self.param_or_zero(params, i + 3) as u8;
                            let b = self.param_or_zero(params, i + 4) as u8;
                            bg = Color::Rgb(r, g, b);
                            i += 4;
                        }
                    }
                }
                49 => bg = Color::Default,
                90..=97 => fg = ansi_bright_fg(param - 90),
                100..=107 => bg = ansi_bright_bg(param - 100),
                _ => {}
            }
            i += 1;
        }

        self.terminal.set_attributes(attrs);
        self.terminal.set_foreground(fg);
        self.terminal.set_background(bg);
    }
}

impl<'a> Perform for AnsiParser<'a> {
    fn print(&mut self, c: char) {
        self.terminal.write(&c.to_string());
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.terminal.write("\n"),
            b'\r' => self.terminal.write("\r"),
            b'\t' => self.terminal.write("\t"),
            b'\x08' => self.terminal.write("\x08"), // backspace
            b'\x07' => {} // bell - ignore
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // Extract cursor y before mutating to avoid borrow conflicts
        let cursor_y = self.terminal.cursor().y;

        // Pre-compute parameters to avoid borrow conflicts
        let p0 = self.param_or_zero(params, 0);
        let p1 = self.param_or_zero(params, 1);

        match action {
            // Cursor movement
            'A' => self.terminal.cursor_mut().move_up(p0 as usize),      // CUU
            'B' => self.terminal.cursor_mut().move_down(p0 as usize),    // CUD
            'C' => self.terminal.cursor_mut().move_right(p0 as usize),   // CUF
            'D' => self.terminal.cursor_mut().move_left(p0 as usize),    // CUB
            'E' => { // CNL - Cursor Next Line
                let y = cursor_y + p0 as usize;
                self.terminal.cursor_mut().move_to(0, y);
            }
            'F' => { // CPL - Cursor Preceding Line
                let y = cursor_y.saturating_sub(p0 as usize);
                self.terminal.cursor_mut().move_to(0, y);
            }
            'G' => { // CHA - Cursor Horizontal Absolute
                let x = p0.saturating_sub(1) as usize;
                self.terminal.cursor_mut().move_to(x, cursor_y);
            }
            'H' | 'f' => { // CUP / HVP - Cursor Position
                let row = p1.saturating_sub(1) as usize;
                let col = self.param_or_zero(params, 1).saturating_sub(1) as usize;
                self.terminal.cursor_mut().move_to(col, row);
            }
            'J' => { // ED - Erase in Display
                self.handle_erase_display(p0);
            }
            'K' => { // EL - Erase in Line
                self.handle_erase_line(p0);
            }
            'S' => { // SU - Scroll Up
                for _ in 0..p0 as usize {
                    self.terminal.grid_mut().scroll_up();
                }
            }
            'T' => { // SD - Scroll Down (not implemented in core yet)
                // TODO: implement scroll_down in Grid
            }
            'h' | 'l' => { // SM/RM - Set/Reset Mode
                // DECCKM, DECCOLM, etc. - ignore for now
            }
            'm' => { // SGR - Select Graphic Rendition
                self.handle_sgr(params);
            }
            'n' => { // DSR - Device Status Report
                // Response would be sent back to PTY - ignore
            }
            'c' => { // DA - Device Attributes
                // Response would be sent back to PTY - ignore
            }
            _ => {} // Unhandled CSI
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell: bool) {
        // OSC sequences - ignore for now
    }
}

/// Mapeia índice ANSI 0-7 para Color padrão
fn ansi_standard_fg(idx: i64) -> Color {
    match idx {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::White,
        _ => Color::Default,
    }
}

/// Mapeia índice ANSI 0-7 para Color brilhante (foreground 90-97)
fn ansi_bright_fg(idx: i64) -> Color {
    match idx {
        0 => Color::BrightBlack,
        1 => Color::BrightRed,
        2 => Color::BrightGreen,
        3 => Color::BrightYellow,
        4 => Color::BrightBlue,
        5 => Color::BrightMagenta,
        6 => Color::BrightCyan,
        7 => Color::BrightWhite,
        _ => Color::Default,
    }
}

/// Mapeia índice ANSI 0-7 para Color de fundo padrão
fn ansi_standard_bg(idx: i64) -> Color {
    ansi_standard_fg(idx)
}

/// Mapeia índice ANSI 0-7 para Color de fundo brilhante
fn ansi_bright_bg(idx: i64) -> Color {
    ansi_bright_fg(idx)
}