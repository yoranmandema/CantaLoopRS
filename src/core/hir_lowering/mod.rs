//! HIR lowering pass: AST → HIR → CompilerState
//! 
//! This module performs the lowering transformation from AST to High-level Intermediate Representation (HIR),
//! and builds CompilerState as the single source of truth for semantic information.
//! It is no longer just "semantic analysis" - it defines symbol identity, scoping, binding, and semantic meaning.

pub mod scopes;
pub mod symbols;
pub mod lower_expr;
pub mod lower_stmt;
pub mod project_semantic_items;

// Re-export core types
pub use scopes::{ScopeId, ScopeIdOld, Scope, ScopeArena, HirBlockContext};
pub use symbols::{SymbolId, TypeId, Symbol, SymbolKind, SymbolTable};
pub use lower_expr::HirExpression;
pub use lower_stmt::{HirStmt, HirBlock, HirBuilder};
pub use project_semantic_items::{SemanticItem, SemanticItemKind, SemanticModifiers};

// Core types that belong in the main module
use std::collections::HashMap;

// AST types are used in submodules, not directly in mod.rs

/// Function signature describing parameter types and return type.
///
/// Used for type checking function calls and declarations.
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    #[allow(dead_code)]
    pub params: Vec<ValueKind>,
    #[allow(dead_code)]
    pub return_type: Box<ValueKind>,
}

/// Represents the type of a value in the CantaLoop type system.
///
/// Includes primitive types (Number, String, Boolean) and
/// function/thunk types with their signatures.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    Number,
    String,
    Boolean,
    Unknown,
    Void,
    // Function type: stores the full type string like "num -> num" or "(num, num) -> num"
    Function(String),
    // Thunk type: stores the full type string like "num ~> num" or "(num, num) ~> num"
    Thunk(String),
}

#[derive(Debug, Clone)]
pub enum ConstantValue {
    Number(f64),
    String(String),
    Boolean(bool),
    #[allow(dead_code)]
    None,
}

#[derive(Debug, Clone)]
pub struct Constant {
    pub id: u32,
    pub name: String,
    pub value: ConstantValue,
    pub kind: ValueKind,
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub id: u32,
    pub name: String,
    pub kind: ValueKind,
}

#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    pub body: HirBlock,
    pub param_var_ids: Vec<u32>, // Variable IDs for parameters, in order
    #[allow(dead_code)]
    pub scope_id: scopes::ScopeId, // The function's scope ID
}

#[derive(Debug, Clone)]
pub struct Function {
    #[allow(dead_code)]
    pub id: u32,
    pub name: String,
    pub signature: FunctionSignature,
    pub definition: FunctionDefinition,
}

/// High-level Intermediate Representation (HIR) of a CantaLoop program.
///
/// HIR is the typed representation after semantic analysis, containing:
/// - Typed expressions and statements
/// - Variable slot assignments
/// - Function definitions with resolved types
/// - Constant table
/// - Import table (imported symbols)
///
/// This is the input to the bytecode compiler.
#[derive(Debug, Clone)]
pub struct HirAst {
    pub constants: Vec<Constant>,
    pub blocks: Vec<HirBlock>,
    pub scopes: ScopeArena,
    pub functions: std::collections::HashMap<u32, Function>, // Function ID -> Function struct
    /// Maps imported symbol names to function IDs (for LSP and symbol resolution)
    pub import_table: ImportTable,
}

impl HirAst {
    /// Get the function signature for a thunk variable to determine total args needed
    /// This helps determine if a thunk will be fully applied
    pub fn get_thunk_function_info(&self, var_id: u32) -> Option<(u32, usize)> {
        // Try to find the original function by looking at the variable's assigned expression
        if let Some(expr) = self.get_var_assigned_expression(var_id) {
            if let HirExpression::FunctionCall {
                function_id,
                args: _,
                ..
            } = expr
            {
                // This is a function call that created the thunk
                if let Some(func) = self.functions.get(function_id) {
                    let total_params = func.signature.params.len();
                    return Some((*function_id, total_params));
                }
            }
        }
        None
    }

    /// Find the expression assigned to a variable (for LSP queries to detect thunks)
    pub fn get_var_assigned_expression(&self, var_id: u32) -> Option<&HirExpression> {
        // Search through all blocks and statements to find the assignment
        for block in &self.blocks {
            for stmt in &block.statements {
                if let HirStmt::Assign { slot, value } = stmt {
                    if *slot == var_id {
                        return Some(value);
                    }
                }
            }
        }
        None
    }
}

/// Source code span (byte offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,  // byte offset
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    
    pub fn length(&self) -> usize {
        self.end - self.start
    }
}

/// Precomputed line index for efficient byte-to-line/column conversion.
/// Built once and reused for all token lookups.
#[derive(Debug, Clone)]
pub struct LineIndex {
    pub line_starts: Vec<usize>,
}

impl LineIndex {
    /// Build a line index from source text.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0]; // First line starts at byte 0
        
        for (byte_pos, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(byte_pos + 1); // Next line starts after newline
            }
        }
        
        Self { line_starts }
    }
    
    /// Lookup line and column from byte offset.
    pub fn lookup(&self, byte: usize) -> (u32, u32) {
        // Find the line containing this byte offset
        let line = self.line_starts.partition_point(|&x| x <= byte) - 1;
        let col = byte - self.line_starts[line];
        (line as u32, col as u32)
    }
}

#[derive(Debug, Clone)]
pub enum HirError {
    #[allow(dead_code)]
    NotImplemented,
    UnknownVariable(String),
    VariableAlreadyDeclared(String),
    TypeMismatch {
        variable: String,
        expected: ValueKind,
        actual: ValueKind,
    },
    TypeError(String),
    BinaryOpTypeError {
        operator: String,
        lhs_type: ValueKind,
        rhs_type: ValueKind,
        expected: String,
    },
    // You can add more specific error variants as needed
}

/// Maps imported symbol names to their function IDs.
/// Used for compile-time resolution of imports.
pub type ImportTable = HashMap<String, u32>; // symbol_name -> function_id

/// Represents a module that can be imported from.
/// A module is a collection of functions and constants identified by dot-separated paths.
pub struct Module {
    /// Functions in this module: function_name -> function_id
    pub functions: HashMap<String, u32>,
    /// Constants in this module: constant_name -> constant_id
    pub constants: HashMap<String, u32>,
}

// Hashable key for constant deduplication
#[derive(Hash, PartialEq, Eq, Clone)]
enum ConstantKey {
    Number(u64), // f64 bit representation
    String(String),
    Boolean(bool),
}

impl ConstantKey {
    fn from_constant_value(value: &ConstantValue) -> Self {
        match value {
            ConstantValue::Number(n) => ConstantKey::Number(n.to_bits()),
            ConstantValue::String(s) => ConstantKey::String(s.clone()),
            ConstantValue::Boolean(b) => ConstantKey::Boolean(*b),
            ConstantValue::None => panic!("Cannot create key from None constant"),
        }
    }
}

/// Compiler state containing all semantic information from compilation.
/// 
/// This is the single source of truth for the compiler's understanding of the program.
/// The LSP consumes this state instead of re-parsing or re-analyzing.
#[derive(Debug, Clone)]
pub struct CompilerState {
    pub ast: crate::core::ast::Program,
    pub hir: HirAst,
    pub diagnostics: Vec<HirError>,
    pub symbols: SymbolTable,
    /// Unified semantic items (keywords, operators, identifiers, types) extracted from AST.
    /// This is the single source for all LSP semantic tokenization - no text scanning needed.
    pub semantic_items: Vec<SemanticItem>,
    /// Precomputed line index for efficient byte-to-line/column conversion.
    pub line_index: Option<LineIndex>,
}

impl CompilerState {
    /// Create a CompilerState from compilation results.
    /// 
    /// This is the single source of truth for the compiler's understanding of the program.
    /// The LSP consumes this state instead of re-parsing or re-analyzing.
    /// 
    /// Note: source text is needed to extract spans, but we don't store it in CompilerState.
    /// If source is None, spans will be None. Semantic items are projected from the state.
    pub fn new(ast: crate::core::ast::Program, hir: HirAst, diagnostics: Vec<HirError>, source: Option<&str>) -> Self {
        let symbols = if let Some(src) = source {
            Self::build_symbol_table(&hir, &ast, src)
        } else {
            Self::build_symbol_table_without_spans(&hir)
        };
        
        let line_index = if let Some(src) = source {
            Some(LineIndex::new(src))
        } else {
            None
        };
        
        // Project semantic items from compiler state (editor projection pass)
        let semantic_items = if let Some(src) = source {
            project_semantic_items::collect_semantic_items(&ast, src, &hir)
        } else {
            Vec::new()
        };
        
        Self {
            ast,
            hir,
            diagnostics,
            symbols,
            semantic_items,
            line_index,
        }
    }
    
    /// Project editor items from compiler state.
    /// 
    /// This is a pure projection function that extracts semantic items (keywords, operators,
    /// identifiers, types) from the compiler state. It does not perform analysis - it only
    /// projects what the compiler already knows into a form suitable for editor tooling.
    /// 
    /// This function is the single source of truth for LSP semantic tokenization.
    /// No text scanning. No heuristics. No divergence from compiler state.
    pub fn project_editor_items(&self, source: &str) -> Vec<SemanticItem> {
        project_semantic_items::collect_semantic_items(&self.ast, source, &self.hir)
    }
    
    // Symbol table building functions (moved from symbols module for now due to dependencies)
    fn build_symbol_table(hir: &HirAst, ast: &crate::core::ast::Program, source: &str) -> SymbolTable {
        symbols::build_symbol_table(hir, ast, source)
    }
    
    fn build_symbol_table_without_spans(hir: &HirAst) -> SymbolTable {
        symbols::build_symbol_table_without_spans(hir)
    }
}

/// Format a function signature as a type string for display.
fn format_function_type_string(sig: &FunctionSignature) -> String {
    // Use HirBuilder's format_value_kind_for_type logic (duplicated here for module-level access)
    let format_kind = |kind: &ValueKind| -> String {
        match kind {
            ValueKind::Number => "num".to_string(),
            ValueKind::String => "string".to_string(),
            ValueKind::Boolean => "bool".to_string(),
            ValueKind::Unknown => "unknown".to_string(),
            ValueKind::Function(ty) => ty.clone(),
            ValueKind::Thunk(ty) => ty.clone(),
            ValueKind::Void => "void".to_string(),
        }
    };
    
    let params: Vec<String> = sig.params.iter()
        .map(|p| format_kind(p))
        .collect();
    
    let param_str = if params.len() == 1 {
        params[0].clone()
    } else {
        format!("({})", params.join(","))
    };
    
    let return_str = format_kind(&sig.return_type);
    format!("{} -> {}", param_str, return_str)
}

// HirBuilder is already re-exported above

