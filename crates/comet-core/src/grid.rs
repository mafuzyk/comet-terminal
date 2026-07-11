//! Buffer de tela: uma grade retangular de células.

use crate::cell::Cell;

/// Buffer de tela do terminal: uma grade retangular de [`Cell`].
///
/// Internamente as células são guardadas em um único `Vec<Cell>` "achatado"
/// (linha após linha), em vez de `Vec<Vec<Cell>>`. Isso evita uma alocação
/// de heap por linha, mantém todas as células contíguas na memória
/// (melhor localidade de cache ao redesenhar a tela inteira) e faz de
/// `resize`/`clear` operações previsíveis em custo. O preço é que o índice
/// de uma célula precisa ser calculado (`y * width + x`) em vez de ser um
/// duplo-índice direto — um cálculo trivial e o compilador otimiza bem.
#[derive(Debug, Clone)]
pub struct Grid {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
}

impl Grid {
    /// Cria uma grade `width` x `height` preenchida com células em branco.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width * height],
        }
    }

    /// Largura da grade, em colunas.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Altura da grade, em linhas.
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Calcula o índice linear de `(x, y)` no buffer interno.
    ///
    /// Não valida limites; chamado apenas depois que os métodos públicos
    /// já checaram `x < width` e `y < height`.
    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    /// Referência à célula em `(x, y)`, ou `None` se estiver fora dos limites.
    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get(self.index(x, y))
    }

    /// Referência mutável à célula em `(x, y)`, ou `None` se fora dos limites.
    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut Cell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = self.index(x, y);
        self.cells.get_mut(idx)
    }

    /// Substitui a célula em `(x, y)`. Retorna `false` se a posição estiver
    /// fora dos limites da grade (nada é alterado nesse caso).
    pub fn set(&mut self, x: usize, y: usize, cell: Cell) -> bool {
        match self.get_mut(x, y) {
            Some(slot) => {
                *slot = cell;
                true
            }
            None => false,
        }
    }

    /// Fatia somente leitura com todas as células da linha `y`.
    pub fn row(&self, y: usize) -> Option<&[Cell]> {
        if y >= self.height {
            return None;
        }
        let start = y * self.width;
        Some(&self.cells[start..start + self.width])
    }

    /// Repõe todas as células da grade para o estado em branco, mantendo
    /// as dimensões atuais.
    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = Cell::default();
        }
    }

    /// Redimensiona a grade, preservando o conteúdo que couber na nova área
    /// (canto superior esquerdo). Colunas/linhas novas são preenchidas com
    /// células em branco; conteúdo que ficar fora da nova área é descartado.
    pub fn resize(&mut self, new_width: usize, new_height: usize) {
        let mut new_cells = vec![Cell::default(); new_width * new_height];

        let copy_width = self.width.min(new_width);
        let copy_height = self.height.min(new_height);

        for y in 0..copy_height {
            let old_start = y * self.width;
            let new_start = y * new_width;
            new_cells[new_start..new_start + copy_width]
                .copy_from_slice(&self.cells[old_start..old_start + copy_width]);
        }

        self.width = new_width;
        self.height = new_height;
        self.cells = new_cells;
    }

    /// Rola a grade uma linha para cima: descarta a linha do topo e insere
    /// uma linha em branco no final.
    ///
    /// Nota de performance: esta implementação desloca todo o buffer
    /// (`O(width * height)`), o que é aceitável para uma tela de terminal
    /// típica (poucos milhares de células) mas não é ideal para scroll muito
    /// frequente. Ver seção "melhorias futuras" na explicação de arquitetura
    /// sobre representar linhas como um ring buffer para scroll em `O(width)`.
    pub fn scroll_up(&mut self) {
        if self.height == 0 || self.width == 0 {
            return;
        }
        self.cells.drain(0..self.width);
        self.cells
            .extend(std::iter::repeat(Cell::default()).take(self.width));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_has_correct_dimensions_and_blank_cells() {
        let grid = Grid::new(4, 3);
        assert_eq!(grid.width(), 4);
        assert_eq!(grid.height(), 3);
        for y in 0..3 {
            for x in 0..4 {
                assert!(grid.get(x, y).unwrap().is_blank());
            }
        }
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let grid = Grid::new(4, 3);
        assert!(grid.get(4, 0).is_none());
        assert!(grid.get(0, 3).is_none());
    }

    #[test]
    fn set_writes_and_get_reads_back() {
        let mut grid = Grid::new(4, 3);
        let cell = Cell {
            character: 'A',
            ..Cell::default()
        };
        assert!(grid.set(1, 1, cell));
        assert_eq!(grid.get(1, 1).unwrap().character, 'A');
        // Vizinhas permanecem em branco.
        assert!(grid.get(0, 1).unwrap().is_blank());
    }

    #[test]
    fn set_out_of_bounds_returns_false_and_does_not_panic() {
        let mut grid = Grid::new(2, 2);
        let cell = Cell {
            character: 'Z',
            ..Cell::default()
        };
        assert!(!grid.set(5, 5, cell));
    }

    #[test]
    fn row_returns_all_cells_in_that_line() {
        let mut grid = Grid::new(3, 2);
        grid.set(
            0,
            1,
            Cell {
                character: 'X',
                ..Cell::default()
            },
        );
        grid.set(
            1,
            1,
            Cell {
                character: 'Y',
                ..Cell::default()
            },
        );
        grid.set(
            2,
            1,
            Cell {
                character: 'Z',
                ..Cell::default()
            },
        );

        let row = grid.row(1).unwrap();
        let chars: Vec<char> = row.iter().map(|c| c.character).collect();
        assert_eq!(chars, vec!['X', 'Y', 'Z']);
    }

    #[test]
    fn resize_preserves_overlapping_content() {
        let mut grid = Grid::new(3, 2);
        grid.set(
            0,
            0,
            Cell {
                character: 'A',
                ..Cell::default()
            },
        );
        grid.set(
            2,
            1,
            Cell {
                character: 'B',
                ..Cell::default()
            },
        );

        // Encolhe: (2, 1) sai da área nova e deve ser descartado.
        grid.resize(2, 2);
        assert_eq!(grid.width(), 2);
        assert_eq!(grid.height(), 2);
        assert_eq!(grid.get(0, 0).unwrap().character, 'A');
        assert!(grid.get(1, 1).unwrap().is_blank());

        // Expande: novas colunas/linhas nascem em branco.
        grid.resize(4, 4);
        assert_eq!(grid.get(0, 0).unwrap().character, 'A');
        assert!(grid.get(3, 3).unwrap().is_blank());
    }

    #[test]
    fn scroll_up_discards_top_row_and_appends_blank_row() {
        let mut grid = Grid::new(2, 2);
        grid.set(
            0,
            0,
            Cell {
                character: 'T',
                ..Cell::default()
            },
        );
        grid.set(
            0,
            1,
            Cell {
                character: 'B',
                ..Cell::default()
            },
        );

        grid.scroll_up();

        // A antiga linha 1 ("B") agora é a linha 0.
        assert_eq!(grid.get(0, 0).unwrap().character, 'B');
        // A última linha é nova e em branco.
        assert!(grid.get(0, 1).unwrap().is_blank());
    }

    #[test]
    fn scroll_up_on_empty_grid_does_not_panic() {
        let mut grid = Grid::new(0, 0);
        grid.scroll_up();
        assert_eq!(grid.width(), 0);
    }
}
