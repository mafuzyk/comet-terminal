//! Sessão de terminal: une grade, cursor e o estado de escrita atual.

use crate::cell::{Attributes, Cell};
use crate::color::Color;
use crate::cursor::Cursor;
use crate::grid::Grid;
use crate::scrollback::{Row, ScrollbackBuffer};
use crate::selection::Selection;
use std::collections::HashMap;

/// Representa uma sessão de terminal completa.
///
/// `Terminal` é o único ponto de entrada pensado para consumidores externos
/// (o parser de ANSI, o PTY, a UI) — eles não devem mexer em [`Grid`] ou
/// [`Cursor`] diretamente na maior parte do tempo. É aqui que moram as
/// regras que dependem de largura *e* altura ao mesmo tempo: quebra de
/// linha automática, rolagem (scroll) e clamping do cursor ao redimensionar,
/// nenhuma das quais poderia viver em `Grid` ou `Cursor` isoladamente sem
/// que um dos dois passasse a conhecer o outro.
///
/// `Terminal` também guarda a "caneta" atual (cor de primeiro plano, cor de
/// fundo e atributos que serão aplicados ao próximo caractere escrito).
/// Isso espelha como terminais reais funcionam: um parser de ANSI recebe
/// `ESC[31m` e chama [`Terminal::set_foreground`], e só então o próximo
/// caractere escrito herda essa cor.
pub struct Terminal {
    width: usize,
    height: usize,
    grid: Grid,
    cursor: Cursor,
    pen_foreground: Color,
    pen_background: Color,
    pen_attributes: Attributes,
    pen_hyperlink: Option<String>,
    hyperlinks: HashMap<(usize, usize), String>,
    scrollback: ScrollbackBuffer,
    viewport_offset: usize, // 0 = at bottom (normal), >0 = scrolled up
    selection: Selection,
    clipboard_output: Option<String>,
    bell_pending: bool,
}

impl Terminal {
    /// Cria uma nova sessão de terminal com a largura e altura dadas
    /// (em colunas e linhas), grade em branco e cursor na origem.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            grid: Grid::new(width, height),
            cursor: Cursor::new(),
            pen_foreground: Color::default(),
            pen_background: Color::default(),
            pen_attributes: Attributes::default(),
            pen_hyperlink: None,
            hyperlinks: HashMap::new(),
            scrollback: ScrollbackBuffer::new(10000, width),
            viewport_offset: 0,
            selection: Selection::new(),
            clipboard_output: None,
            bell_pending: false,
        }
    }

    /// Creates a new terminal with custom scrollback size.
    pub fn with_scrollback(width: usize, height: usize, scrollback_size: usize) -> Self {
        Self {
            width,
            height,
            grid: Grid::new(width, height),
            cursor: Cursor::new(),
            pen_foreground: Color::default(),
            pen_background: Color::default(),
            pen_attributes: Attributes::default(),
            pen_hyperlink: None,
            hyperlinks: HashMap::new(),
            scrollback: ScrollbackBuffer::new(scrollback_size, width),
            viewport_offset: 0,
            selection: Selection::new(),
            clipboard_output: None,
            bell_pending: false,
        }
    }

    /// Largura atual, em colunas.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Altura atual, em linhas.
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Acesso somente leitura à grade de células, para a camada de
    /// renderização percorrer e desenhar.
    pub const fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Acesso somente leitura ao cursor.
    pub const fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// Acesso mutável ao cursor, para comandos explícitos de posicionamento
    /// vindos, por exemplo, de sequências ANSI de movimento de cursor.
    pub fn cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursor
    }

    /// Acesso mutável à grade (para parser ANSI escrever células diretamente).
    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    /// Cor de primeiro plano atual da caneta.
    pub fn pen_foreground(&self) -> Color {
        self.pen_foreground
    }

    /// Cor de fundo atual da caneta.
    pub fn pen_background(&self) -> Color {
        self.pen_background
    }

    /// Atributos atuais da caneta.
    pub fn pen_attributes(&self) -> Attributes {
        self.pen_attributes
    }

    /// Define a cor de primeiro plano usada pelos próximos caracteres
    /// escritos via [`write`](Terminal::write).
    pub fn set_foreground(&mut self, color: Color) {
        self.pen_foreground = color;
    }

    /// Define a cor de fundo usada pelos próximos caracteres escritos.
    pub fn set_background(&mut self, color: Color) {
        self.pen_background = color;
    }

    /// Define os atributos de estilo usados pelos próximos caracteres
    /// escritos.
    pub fn set_attributes(&mut self, attributes: Attributes) {
        self.pen_attributes = attributes;
    }

    /// Restaura a "caneta" (cores e atributos) para os valores padrão.
    /// Equivalente ao `ESC[0m` do ANSI.
    pub fn reset_pen(&mut self) {
        self.pen_foreground = Color::default();
        self.pen_background = Color::default();
        self.pen_attributes = Attributes::default();
    }

    /// Escreve texto simples na posição atual do cursor, avançando-o
    /// caractere a caractere.
    ///
    /// Este método **não interpreta sequências de escape ANSI** — isso é
    /// responsabilidade de uma camada futura que traduz `ESC[...]` em
    /// chamadas a esta API (`set_foreground`, `cursor_mut().move_to`, etc).
    /// Aqui só existem três comportamentos especiais, os mesmos que
    /// qualquer terminal aplica a texto puro:
    ///
    /// - `\n` move para a próxima linha (rolando a tela se necessário);
    /// - `\r` volta o cursor para a coluna 0;
    /// - qualquer outro caractere é desenhado na grade com a caneta atual
    ///   e o cursor avança uma coluna, quebrando a linha automaticamente
    ///   ao atingir a borda direita.
    pub fn write(&mut self, text: &str) {
        for ch in text.chars() {
            self.write_char(ch);
        }
    }

    fn write_char(&mut self, ch: char) {
        match ch {
            '\n' => self.newline(),
            '\r' => self.cursor.x = 0,
            _ => {
                let mut attrs = self.pen_attributes;
                let has_hyperlink = self.pen_hyperlink.is_some();
                if has_hyperlink {
                    attrs.hyperlink = true;
                }
                let cell = Cell {
                    character: ch,
                    foreground: self.pen_foreground,
                    background: self.pen_background,
                    attributes: attrs,
                };
                let abs_pos = (self.scrollback.len() + self.cursor.y, self.cursor.x);
                self.grid.set(self.cursor.x, self.cursor.y, cell);
                if has_hyperlink {
                    if let Some(uri) = &self.pen_hyperlink {
                        self.hyperlinks.insert(abs_pos, uri.clone());
                    }
                }
                self.advance_cursor();
            }
        }
    }

    /// Avança o cursor uma coluna, quebrando para a próxima linha se
    /// atingir a borda direita da grade.
    fn advance_cursor(&mut self) {
        if self.width == 0 {
            return;
        }
        self.cursor.x += 1;
        if self.cursor.x >= self.width {
            self.cursor.x = 0;
            self.newline();
        }
    }

    /// Move o cursor para a próxima linha, rolando a grade para cima
    /// quando o cursor já está na última linha.
    /// Pushes the scrolled-off line to scrollback before scrolling.
    fn newline(&mut self) {
        if self.height == 0 {
            return;
        }
        if self.cursor.y + 1 >= self.height {
            // Capture the top line before it scrolls off
            if let Some(top_row) = self.grid.row(0) {
                self.scrollback.push_line(Row::from_cells(top_row));
            }
            self.grid.scroll_up();
            // If we're scrolled up, maintain the same visual position
            if self.viewport_offset > 0 {
                self.viewport_offset += 1;
            }
        } else {
            self.cursor.y += 1;
        }
    }

    /// Redimensiona o terminal, preservando o conteúdo da grade que couber
    /// na nova área e reposicionando o cursor caso ele fique fora dos
    /// novos limites.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.grid.resize(width, height);
        self.cursor.x = self.cursor.x.min(width.saturating_sub(1));
        self.cursor.y = self.cursor.y.min(height.saturating_sub(1));
    }

    /// Limpa toda a tela e retorna o cursor para a origem `(0, 0)`.
    /// A caneta atual (cores/atributos) não é afetada.
    pub fn clear(&mut self) {
        self.grid.clear();
        self.cursor.move_to(0, 0);
    }

    // ========== Scrollback & Viewport Methods ==========

    /// Returns a reference to the scrollback buffer.
    pub fn scrollback(&self) -> &ScrollbackBuffer {
        &self.scrollback
    }

    /// Returns a mutable reference to the scrollback buffer.
    pub fn scrollback_mut(&mut self) -> &mut ScrollbackBuffer {
        &mut self.scrollback
    }

    /// Returns the current viewport offset (0 = at bottom, >0 = scrolled up).
    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    /// Returns true if the viewport is at the bottom (showing live output).
    pub fn is_at_bottom(&self) -> bool {
        self.viewport_offset == 0
    }

    /// Scrolls the viewport up by `amount` lines.
    /// Returns the actual number of lines scrolled.
    pub fn scroll_viewport_up(&mut self, amount: usize) -> usize {
        let scrolled = self.scrollback.scroll_up(amount);
        self.viewport_offset += scrolled;
        scrolled
    }

    /// Scrolls the viewport down by `amount` lines.
    /// Returns the actual number of lines scrolled.
    pub fn scroll_viewport_down(&mut self, amount: usize) -> usize {
        let scrolled = self.scrollback.scroll_down(amount);
        self.viewport_offset -= scrolled;
        scrolled
    }

    /// Scrolls the viewport to the top of the scrollback.
    pub fn scroll_viewport_to_top(&mut self) {
        self.scrollback.scroll_to_top();
        self.viewport_offset = self.scrollback.viewport_offset();
    }

    /// Scrolls the viewport to the bottom (live output).
    pub fn scroll_viewport_to_bottom(&mut self) {
        self.scrollback.scroll_to_bottom();
        self.viewport_offset = 0;
    }

    /// Scrolls the viewport by a number of pages (heights of lines (positive = up, negative = down).
    pub fn scroll_viewport_pages(&mut self, pages: isize) {
        let page_size = self.height.saturating_sub(1).max(1);
        let lines = (pages * page_size as isize).abs() as usize;
        if pages > 0 {
            self.scroll_viewport_up(lines);
        } else if pages < 0 {
            self.scroll_viewport_down(lines);
        }
    }

    /// Returns the visible rows for rendering, accounting for viewport offset.
    /// Returns a vector of rows (owned), starting from the top of the viewport.
    /// When viewport_offset > 0, scrollback history is shown above the grid.
    /// When viewport_offset == 0 (at bottom), only grid rows are returned.
    pub fn visible_rows(&self) -> Vec<Row> {
        let mut rows = Vec::with_capacity(self.height);
        self.fill_visible_rows(&mut rows);
        rows
    }

    /// Fills an existing buffer with visible rows, reusing its allocation.
    /// The buffer is cleared and refilled with the current visible rows.
    /// This avoids allocating a new Vec each frame.
    pub fn fill_visible_rows(&self, rows: &mut Vec<Row>) {
        rows.clear();
        rows.reserve(self.height);
        let sb_len = self.scrollback.len();
        let n_sb = self.viewport_offset.min(sb_len);

        // Scrollback lines: oldest first (top of viewport)
        for i in 0..n_sb {
            let sb_idx = n_sb - 1 - i;
            if let Some(row) = self.scrollback.get_line(sb_idx) {
                rows.push(row.clone());
            } else {
                rows.push(Row::new(self.width));
            }
        }

        // Grid lines fill the rest
        for i in n_sb..self.height {
            let grid_row = i - n_sb;
            if let Some(cells) = self.grid.row(grid_row) {
                rows.push(Row::from_cells(cells));
            } else {
                rows.push(Row::new(self.width));
            }
        }
    }

    /// Converts a visible (screen) row index to an absolute row index
    /// (suitable for use with `get_cell_absolute` and selection methods).
    pub fn visible_row_to_absolute(&self, visible_row: usize) -> usize {
        let sb_len = self.scrollback.len();
        let n_sb = self.viewport_offset.min(sb_len);
        if visible_row < n_sb {
            n_sb - 1 - visible_row
        } else {
            sb_len + visible_row - n_sb
        }
    }

    /// Converts an absolute row index to a visible (screen) row index.
    /// Returns `None` if the absolute row is outside the visible viewport.
    pub fn absolute_to_visible_row(&self, absolute_row: usize) -> Option<usize> {
        let sb_len = self.scrollback.len();
        let n_sb = self.viewport_offset.min(sb_len);
        // Scrollback portion: absolute rows 0..n_sb are visible in reverse
        if absolute_row < n_sb {
            return Some(n_sb - 1 - absolute_row);
        }
        // Grid portion: absolute rows sb_len..sb_len+height-n_sb are visible
        let grid_start = sb_len;
        let grid_end = sb_len + self.height - n_sb;
        if absolute_row >= grid_start && absolute_row < grid_end {
            return Some(absolute_row - sb_len + n_sb);
        }
        None
    }

    /// Gets a cell at the given absolute position (including scrollback).
    /// x: column, y: absolute row (0 = top of scrollback).
    pub fn get_cell_absolute(&self, x: usize, y: usize) -> Option<&Cell> {
        if y < self.scrollback.len() {
            self.scrollback.get_line(y).and_then(|r| r.cells.get(x))
        } else {
            let grid_y = y - self.scrollback.len();
            if grid_y < self.height {
                self.grid.get(x, grid_y)
            } else {
                None
            }
        }
    }

    // ========== Selection Methods ==========

    /// Returns a reference to the current selection.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Returns a mutable reference to the selection.
    pub fn selection_mut(&mut self) -> &mut Selection {
        &mut self.selection
    }

    /// Starts a new selection at the given position.
    pub fn start_selection(&mut self, col: usize, row: usize) {
        self.selection.start(col, row);
    }

    /// Updates the selection end position.
    pub fn update_selection(&mut self, col: usize, row: usize) {
        self.selection.update(col, row);
    }

    /// Ends the current selection.
    pub fn end_selection(&mut self) {
        self.selection.end();
    }

    /// Clears the current selection.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Returns true if there is an active selection.
    pub fn has_selection(&self) -> bool {
        self.selection.is_active()
    }

    /// Returns the selected text as a string.
    pub fn get_selection_text(&self) -> String {
        self.selection
            .get_text(|col, row| self.get_cell_absolute(col, row).copied())
    }

    // ========== Hyperlink Methods ==========

    /// Sets the current hyperlink URI (from OSC 8 sequence).
    /// Pass `None` to end the hyperlink.
    pub fn set_hyperlink(&mut self, uri: Option<String>) {
        self.pen_hyperlink = uri;
    }

    /// Returns the hyperlink URI at the given absolute position, if any.
    pub fn get_hyperlink(&self, abs_row: usize, col: usize) -> Option<&str> {
        self.hyperlinks.get(&(abs_row, col)).map(|s| s.as_str())
    }

    /// Returns the current pen hyperlink URI.
    pub fn pen_hyperlink(&self) -> Option<&str> {
        self.pen_hyperlink.as_deref()
    }

    // ========== Clipboard (OSC 52) ==========

    /// Sets clipboard output content from OSC 52 sequence.
    pub fn set_clipboard_output(&mut self, content: Option<String>) {
        self.clipboard_output = content;
    }

    /// Takes the clipboard output content (clears it).
    pub fn take_clipboard_output(&mut self) -> Option<String> {
        self.clipboard_output.take()
    }

    // ========== Bell ==========

    /// Returns true if a bell (BEL) is pending.
    pub fn is_bell_pending(&self) -> bool {
        self.bell_pending
    }

    /// Marks a bell as pending.
    pub fn mark_bell(&mut self) {
        self.bell_pending = true;
    }

    /// Clears the pending bell flag.
    pub fn clear_bell(&mut self) {
        self.bell_pending = false;
    }

    /// Expands the current selection to word boundaries.
    pub fn expand_selection_to_word(&mut self) {
        // Extract the bounds first to avoid borrow conflict
        let bounds = self.selection.bounds();
        if let Some((start_col, start_row, end_col, end_row)) = bounds {
            // Expand start backward to word boundary
            let mut col = start_col;
            while col > 0 {
                if let Some(cell) = self.get_cell_absolute(col - 1, start_row).copied() {
                    let ch = cell.character;
                    if ch.is_alphanumeric() || ch == '_' {
                        col -= 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            let new_start_col = col;

            // Expand end forward to word boundary
            let mut col = end_col;
            while let Some(cell) = self.get_cell_absolute(col, end_row).copied() {
                let ch = cell.character;
                if ch.is_alphanumeric() || ch == '_' {
                    col += 1;
                } else {
                    break;
                }
            }
            let new_end_col = col;

            self.selection.set_start(new_start_col, start_row);
            self.selection.set_end(new_end_col, end_row);
        }
    }

    /// Expands the current selection to full lines.
    pub fn expand_selection_to_line(&mut self) {
        self.selection.expand_to_line();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_text(terminal: &Terminal, y: usize) -> String {
        terminal
            .grid()
            .row(y)
            .unwrap()
            .iter()
            .map(|c| c.character)
            .collect()
    }

    #[test]
    fn new_terminal_has_blank_grid_and_cursor_at_origin() {
        let terminal = Terminal::new(80, 24);
        assert_eq!(terminal.width(), 80);
        assert_eq!(terminal.height(), 24);
        assert_eq!(terminal.cursor().position(), (0, 0));
    }

    #[test]
    fn write_places_characters_and_advances_cursor() {
        let mut terminal = Terminal::new(80, 24);
        terminal.write("Hi");
        assert_eq!(&row_text(&terminal, 0)[0..2], "Hi");
        assert_eq!(terminal.cursor().position(), (2, 0));
    }

    #[test]
    fn write_applies_current_pen_colors() {
        let mut terminal = Terminal::new(10, 2);
        terminal.set_foreground(Color::Red);
        terminal.write("A");
        let cell = terminal.grid().get(0, 0).unwrap();
        assert_eq!(cell.character, 'A');
        assert_eq!(cell.foreground, Color::Red);
    }

    #[test]
    fn carriage_return_moves_cursor_to_column_zero() {
        let mut terminal = Terminal::new(10, 2);
        terminal.write("abc\r");
        assert_eq!(terminal.cursor().position(), (0, 0));
    }

    /// `\n` sozinho é *line feed* puro: como em um terminal real (VT100/xterm),
    /// ele move o cursor uma linha para baixo mas **preserva a coluna atual**.
    /// Quem quiser "linha nova completa" precisa mandar `\r\n`, exatamente como
    /// faz um PTY em modo canônico ou uma aplicação bem-comportada.
    #[test]
    fn line_feed_alone_moves_down_but_keeps_column() {
        let mut terminal = Terminal::new(10, 3);
        terminal.write("ab");
        terminal.write("\n");
        assert_eq!(terminal.cursor().position(), (2, 1));

        terminal.write("cd");
        // "cd" foi escrito a partir da coluna 2 da linha 1, não da coluna 0.
        assert_eq!(&row_text(&terminal, 1)[2..4], "cd");
        assert_eq!(terminal.cursor().position(), (4, 1));
    }

    #[test]
    fn carriage_return_then_line_feed_starts_next_line_at_column_zero() {
        let mut terminal = Terminal::new(10, 3);
        terminal.write("ab\r\ncd");
        assert_eq!(terminal.cursor().position(), (2, 1));
        assert_eq!(&row_text(&terminal, 0)[0..2], "ab");
        assert_eq!(&row_text(&terminal, 1)[0..2], "cd");
    }

    #[test]
    fn writing_past_last_column_wraps_to_next_line() {
        let mut terminal = Terminal::new(3, 3);
        terminal.write("abcd");
        assert_eq!(&row_text(&terminal, 0), "abc");
        assert_eq!(&row_text(&terminal, 1)[0..1], "d");
        assert_eq!(terminal.cursor().position(), (1, 1));
    }

    #[test]
    fn writing_past_last_row_scrolls_up() {
        // Largura 3 (maior que o conteúdo) isola o comportamento de scroll
        // do de auto-wrap testado separadamente acima.
        let mut terminal = Terminal::new(3, 2);
        // Preenche as duas linhas e escreve uma terceira: deve rolar.
        terminal.write("ab\r\ncd\r\nef");
        assert_eq!(&row_text(&terminal, 0)[0..2], "cd");
        assert_eq!(&row_text(&terminal, 1)[0..2], "ef");
    }

    #[test]
    fn resize_preserves_content_and_clamps_cursor() {
        let mut terminal = Terminal::new(5, 5);
        terminal.write("Hello");
        terminal.cursor_mut().move_to(4, 4);

        terminal.resize(3, 3);

        assert_eq!(terminal.width(), 3);
        assert_eq!(terminal.height(), 3);
        assert_eq!(&row_text(&terminal, 0), "Hel");
        // Cursor estava em (4, 4), fora da nova área 3x3: deve ser preso a (2, 2).
        assert_eq!(terminal.cursor().position(), (2, 2));
    }

    #[test]
    fn clear_blanks_grid_and_resets_cursor() {
        let mut terminal = Terminal::new(5, 5);
        terminal.write("Hello");
        terminal.clear();

        assert_eq!(terminal.cursor().position(), (0, 0));
        assert!(terminal.grid().get(0, 0).unwrap().is_blank());
    }

    #[test]
    fn reset_pen_restores_default_colors_and_attributes() {
        let mut terminal = Terminal::new(5, 5);
        terminal.set_foreground(Color::Red);
        terminal.set_attributes(Attributes {
            bold: true,
            ..Attributes::default()
        });
        terminal.reset_pen();
        terminal.write("A");

        let cell = terminal.grid().get(0, 0).unwrap();
        assert_eq!(cell.foreground, Color::Default);
        assert_eq!(cell.attributes, Attributes::default());
    }
}
