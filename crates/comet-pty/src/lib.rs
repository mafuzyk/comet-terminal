//! # comet-pty
//!
//! Camada de PTY + parser ANSI para o Comet Terminal.
//!
//! Conecta um shell real (via `portable-pty`) ao estado do terminal
//! (`comet-core::Terminal`), processando sequências de escape ANSI
//! através de `vte` e aplicando as mutações na grade/cursor/caneta.

mod parser;
mod pty;
mod terminal;

pub use parser::AnsiParser;
pub use pty::{PtyConfig, PtyError, PtyProcess};
pub use terminal::PtyTerminal;
