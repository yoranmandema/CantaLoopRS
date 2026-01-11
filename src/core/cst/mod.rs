/// Concrete Syntax Tree (CST) module.
/// 
/// The CST preserves exact source spans and represents the syntax structure
/// of parsed code without semantic validation.
/// 
/// The CST is lowered to AST/HIR for semantic analysis and execution.

mod span;
mod node_id;
mod enums;
mod builder;
mod lowering;
mod pratt;

pub use span::{Span, Spanned};
pub use node_id::{CstId, CstIdGenerator};
pub use enums::*;
pub use builder::build_cst_program;
pub use lowering::lower_cst_to_ast;

use crate::core::parser::{CantaLoopParser, Rule};
use pest::Parser as PestParser;

/// Parses CantaLoop source code into a Concrete Syntax Tree.
/// 
/// The CST preserves exact source spans for LSP integration.
/// Use `lower_cst_to_ast` to convert to AST for semantic analysis.
/// 
/// Returns both the CST and a map of documentation blocks keyed by declaration span.
pub fn parse_cst_program(src: &str) -> Result<(CstProgram, std::collections::HashMap<Span, DocBlock>), pest::error::Error<Rule>> {
    let parse_result = CantaLoopParser::parse(Rule::program, src);
    let mut pairs = parse_result?;
    // Recover from missing program pair - create error at start of source
    let program = pairs.next().ok_or_else(|| {
        // Create position at start of source - pest::Position::new should always succeed for valid source strings
        // If src is empty, position 0 is still valid
        let pos = pest::Position::new(src, 0)
            .or_else(|| pest::Position::new("", 0))
            .expect("Should always be able to create position at index 0");
        pest::error::Error::new_from_pos(
            pest::error::ErrorVariant::CustomError {
                message: "No program found in parse result".to_string(),
            },
            pos
        )
    })?;
    build_cst_program(program)
}

