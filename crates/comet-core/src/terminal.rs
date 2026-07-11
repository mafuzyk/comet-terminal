//! Sessão de terminal: une grade, cursor e o estado de escrita atual.

use crate::cell::{Attributes, Cell};
use crate::color::Color;
use crate::cursor::Cursor;
use crate::grid::Grid;

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
                let cell = Cell {
                    character: ch,
                    foreground: self.pen_foreground,
                    background: self.pen_background,
                    attributes: self.pen_attributes,
                };
                self.grid.set(self.cursor.x, self.cursor.y, cell);
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
    fn newline(&mut self) {
        if self.height == 0 {
            return;
        }
        if self.cursor.y + 1 >= self.height {
            self.grid.scroll_up();
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
