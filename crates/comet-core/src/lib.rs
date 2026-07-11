//! # comet-core
//!
//! Núcleo independente de UI do Comet Terminal.
//!
//! Este crate representa apenas o **estado** de uma sessão de terminal:
//! uma grade de células, um cursor e a "caneta" (cor/atributos) atual.
//! Ele não sabe nada sobre janelas, Wayland, GPU, PTY, shell ou sequências
//! de escape ANSI — essas responsabilidades pertencem a outras camadas do
//! Comet (`comet-renderer`, `comet-ui`, `comet`) que consomem esta API.
//!
//! ## Exemplo
//!
//! ```
//! use comet_core::Terminal;
//!
//! let mut terminal = Terminal::new(80, 24);
//! terminal.write("Hello Comet");
//!
//! assert_eq!(terminal.cursor().position(), (11, 0));
//! ```
//!
//! ## Estrutura
//!
//! - [`Terminal`] — ponto de entrada principal; une grade, cursor e caneta.
//! - [`Grid`] — buffer de tela: uma matriz de [`Cell`].
//! - [`Cell`] — uma posição da grade: caractere, cores e atributos.
//! - [`Cursor`] — posição e visibilidade do cursor de texto.
//! - [`Color`] — cor de primeiro plano/fundo (nomeada, indexada ou RGB).

mod cell;
mod color;
mod cursor;
mod grid;
mod terminal;

pub use cell::{Attributes, Cell};
pub use color::Color;
pub use cursor::Cursor;
pub use grid::Grid;
pub use terminal::Terminal;
