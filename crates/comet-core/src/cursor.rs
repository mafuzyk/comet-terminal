//! Abstração de posição e visibilidade do cursor do terminal.

/// Posição e visibilidade do cursor de texto.
///
/// `Cursor` propositalmente **não conhece** as dimensões da grade. Ele só
/// guarda um par de coordenadas e responde a comandos de movimento; quem
/// garante que o cursor não saia dos limites da tela é [`Terminal`],
/// que é o único componente que conhece largura e altura simultaneamente.
/// Isso mantém `Cursor` pequeno, sem dependências e trivial de testar
/// isoladamente.
///
/// Coordenadas são baseadas em zero: `(0, 0)` é o canto superior esquerdo.
///
/// [`Terminal`]: crate::Terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
    visible: bool,
}

impl Cursor {
    /// Cria um cursor na origem `(0, 0)`, visível.
    pub const fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            visible: true,
        }
    }

    /// Posição atual como tupla `(x, y)`.
    pub const fn position(&self) -> (usize, usize) {
        (self.x, self.y)
    }

    /// Move o cursor para uma posição absoluta.
    ///
    /// Não faz nenhum clamping: cabe à camada superior ([`Terminal`](crate::Terminal))
    /// validar contra as dimensões da grade.
    pub fn move_to(&mut self, x: usize, y: usize) {
        self.x = x;
        self.y = y;
    }

    /// Move `n` colunas para a esquerda, saturando em `0`.
    pub fn move_left(&mut self, n: usize) {
        self.x = self.x.saturating_sub(n);
    }

    /// Move `n` colunas para a direita. Não satura contra a largura da
    /// grade — ver nota de módulo sobre responsabilidades de `Terminal`.
    pub fn move_right(&mut self, n: usize) {
        self.x = self.x.saturating_add(n);
    }

    /// Move `n` linhas para cima, saturando em `0`.
    pub fn move_up(&mut self, n: usize) {
        self.y = self.y.saturating_sub(n);
    }

    /// Move `n` linhas para baixo. Não satura contra a altura da grade.
    pub fn move_down(&mut self, n: usize) {
        self.y = self.y.saturating_add(n);
    }

    /// Torna o cursor visível.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Oculta o cursor (ex.: durante operações de escrita em lote).
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Retorna `true` se o cursor está atualmente visível.
    pub const fn is_visible(&self) -> bool {
        self.visible
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cursor_starts_at_origin_and_visible() {
        let cursor = Cursor::new();
        assert_eq!(cursor.position(), (0, 0));
        assert!(cursor.is_visible());
    }

    #[test]
    fn move_to_sets_absolute_position() {
        let mut cursor = Cursor::new();
        cursor.move_to(10, 5);
        assert_eq!(cursor.position(), (10, 5));
    }

    #[test]
    fn move_left_and_up_saturate_at_zero() {
        let mut cursor = Cursor::new();
        cursor.move_left(3);
        cursor.move_up(3);
        assert_eq!(cursor.position(), (0, 0));
    }

    #[test]
    fn move_right_and_down_accumulate() {
        let mut cursor = Cursor::new();
        cursor.move_right(4);
        cursor.move_down(2);
        assert_eq!(cursor.position(), (4, 2));
    }

    #[test]
    fn show_and_hide_toggle_visibility() {
        let mut cursor = Cursor::new();
        cursor.hide();
        assert!(!cursor.is_visible());
        cursor.show();
        assert!(cursor.is_visible());
    }
}
