//! Representação de cores usadas por células do terminal.

/// Cor de primeiro plano ou de fundo de uma [`Cell`](crate::Cell).
///
/// O design é intencionalmente "flat" (sem enums aninhados) para refletir
/// exatamente as três formas como um terminal real precisa expressar cor:
///
/// - As 16 cores nomeadas clássicas do ANSI (8 normais + 8 "bright");
/// - Um índice na paleta estendida de 256 cores;
/// - Uma cor RGB de 24 bits (true color).
///
/// `Color` é `Copy`, então armazená-la em cada [`Cell`](crate::Cell) da grade
/// não gera nenhuma alocação de heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    /// Nenhuma cor explícita: a camada de renderização deve aplicar a cor
    /// padrão configurada pelo usuário (equivalente ao "reset" do ANSI).
    #[default]
    Default,

    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,

    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,

    /// Índice de 0-255 na paleta de 256 cores.
    Indexed(u8),

    /// Cor RGB de 24 bits (true color).
    Rgb(u8, u8, u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_color_is_default_variant() {
        assert_eq!(Color::default(), Color::Default);
    }

    #[test]
    fn colors_are_comparable() {
        assert_eq!(Color::White, Color::White);
        assert_ne!(Color::White, Color::Black);
        assert_eq!(Color::Rgb(1, 2, 3), Color::Rgb(1, 2, 3));
        assert_ne!(Color::Indexed(5), Color::Indexed(6));
    }

    #[test]
    fn color_is_copy() {
        // Se isto compila, Color é Copy (não move o valor original).
        let a = Color::Rgb(10, 20, 30);
        let b = a;
        assert_eq!(a, b);
    }
}
