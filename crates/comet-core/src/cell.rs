//! Representação de uma única célula da grade do terminal.

use crate::color::Color;

/// Atributos de estilo básicos aplicáveis a uma célula.
///
/// Cada atributo é um `bool` simples em vez de um conjunto de bitflags:
/// para o punhado de atributos que um terminal precisa (negrito, itálico,
/// sublinhado, riscado, reverso, esmaecido), uma struct de bools é tão
/// barata quanto bitflags em termos de tamanho e cópia (a struct inteira
/// cabe em 1 byte com o layout padrão do compilador) e é mais direta de
/// ler e testar. Se no futuro isso crescer (ex.: sublinhado colorido,
/// múltiplos estilos de sublinhado), migrar para bitflags é uma refatoração
/// isolada nesta struct, sem afetar o restante do core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attributes {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reverse: bool,
    pub dim: bool,
    /// Cell is part of a hyperlink (actual URI tracked separately in Terminal).
    pub hyperlink: bool,
}

/// Uma única posição da grade do terminal: um caractere e seu estilo.
///
/// `Cell` é `Copy` de propósito. Uma grade 80x24 tem 1920 células, e
/// operações como `clear()`, `scroll_up()` ou redimensionamento precisam
/// copiar/mover muitas células por frame; manter `Cell` pequena e `Copy`
/// evita qualquer alocação ou contagem de referência nesse caminho quente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// O caractere exibido nesta célula.
    pub character: char,
    /// Cor de primeiro plano (do texto).
    pub foreground: Color,
    /// Cor de fundo da célula.
    pub background: Color,
    /// Atributos de estilo (negrito, itálico, etc).
    pub attributes: Attributes,
}

impl Cell {
    /// Cria uma célula em branco (espaço) com cores padrão e sem atributos.
    ///
    /// Esta é a célula usada para preencher a grade recém-criada e para
    /// limpar linhas/telas.
    pub const fn blank() -> Self {
        Self {
            character: ' ',
            foreground: Color::Default,
            background: Color::Default,
            attributes: Attributes {
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
                reverse: false,
                dim: false,
                hyperlink: false,
            },
        }
    }

    /// Retorna `true` se a célula é visualmente equivalente a uma célula em
    /// branco (espaço, sem cor ou atributo aplicado). Útil para otimizações
    /// de renderização que queiram pular células "vazias".
    pub fn is_blank(&self) -> bool {
        *self == Self::blank()
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cell_is_a_blank_space() {
        let cell = Cell::default();
        assert_eq!(cell.character, ' ');
        assert_eq!(cell.foreground, Color::Default);
        assert_eq!(cell.background, Color::Default);
        assert_eq!(cell.attributes, Attributes::default());
    }

    #[test]
    fn is_blank_detects_default_cell() {
        assert!(Cell::default().is_blank());

        let cell = Cell {
            character: 'A',
            ..Cell::default()
        };
        assert!(!cell.is_blank());
    }

    #[test]
    fn cell_is_copy() {
        let a = Cell {
            character: 'x',
            ..Cell::default()
        };
        let b = a;
        assert_eq!(a, b);
    }
}
