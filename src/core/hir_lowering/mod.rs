//! HIR lowering pass: AST → HIR → CompilerState
//!
//! This module performs the lowering transformation from AST to High-level Intermediate Representation (HIR),
//! and builds CompilerState as the single source of truth for semantic information.
//! It is no longer just "semantic analysis" - it defines symbol identity, scoping, binding, and semantic meaning.

pub mod lower_expr;
pub mod lower_stmt;
pub mod project_semantic_items;
pub mod scopes;
pub mod symbols;

// Re-export core types
pub use lower_expr::{HirExpression, ReducerType};
pub use lower_stmt::{HirBlock, HirBuilder, HirStmt};
pub use project_semantic_items::{SemanticItem, SemanticItemKind, SemanticModifiers};
pub use scopes::{HirBlockContext, Scope, ScopeArena, ScopeId, ScopeIdOld};
use serde::Serialize;
pub use symbols::{Symbol, SymbolId, SymbolKind, SymbolTable, TypeId};

// Core types that belong in the main module
use std::collections::HashMap;

// AST types are used in submodules, not directly in mod.rs

/// Function signature describing parameter types and return type.
///
/// Used for type checking function calls and declarations.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ValueKind {
    Any,
    Number,
    String,
    Boolean,
    Unknown,
    Void,
    // Function type: stores the full type string like "num -> num" or "(num, num) -> num"
    Function(String),
    // Thunk type: stores the full type string like "num ~> num" or "(num, num) ~> num"
    Thunk(String),
    // Array type: stores the inner element type
    Array(Box<ValueKind>),
    // Struct type: stores the struct name (structs are identified by name)
    Struct(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ConstantValue {
    Number(f64),
    String(String),
    Boolean(bool),
    #[allow(dead_code)]
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct Constant {
    pub id: u32,
    pub name: String,
    pub value: ConstantValue,
    pub kind: ValueKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct Variable {
    pub id: u32,
    pub name: String,
    pub kind: ValueKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionDefinition {
    pub body: HirBlock,
    pub param_var_ids: Vec<u32>, // Variable IDs for parameters, in order
    #[allow(dead_code)]
    pub scope_id: scopes::ScopeId, // The function's scope ID
}

#[derive(Debug, Clone, Serialize)]
pub struct Function {
    #[allow(dead_code)]
    pub id: u32,
    pub name: String,
    pub signature: FunctionSignature,
    pub definition: FunctionDefinition,
}

/// Struct definition (compile-time only).
///
/// Structs do not exist at runtime — only instances do.
#[derive(Debug, Clone, Serialize)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, ValueKind)>, // (field_name, field_type)
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
#[derive(Debug, Clone, Serialize)]
pub struct HirAst {
    pub constants: Vec<Constant>,
    pub blocks: Vec<HirBlock>,
    pub scopes: ScopeArena,
    pub functions: std::collections::HashMap<u32, Function>, // Function ID -> Function struct
    pub structs: HashMap<String, StructDef>, // Struct name -> StructDef
    pub module_imports: HashMap<String, ImportTable>,
    /// Maps imported constant names to their values (for LSP hover)
    pub imported_constant_values: HashMap<String, ConstantValue>,
}


impl Default for HirAst {
    fn default() -> Self {
        HirAst {
            constants: Vec::new(),
            blocks: Vec::new(),
            scopes: ScopeArena { scopes: Vec::new() },
            functions: HashMap::new(),
            structs: HashMap::new(),
            module_imports: HashMap::new(),
            imported_constant_values: HashMap::new(),
        }
    }
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

    /// Get all imports from all modules as a flat iterator
    pub fn all_imports(&self) -> impl Iterator<Item = (&String, &u32)> {
        self.module_imports
            .values()
            .flat_map(|imports| imports.iter())
    }

    /// Get all imported function names across all modules
    pub fn all_imported_function_names(&self) -> impl Iterator<Item = &String> {
        self.all_imports()
            .filter(|(_, func_id)| self.functions.contains_key(func_id))
            .map(|(name, _)| name)
    }

    /// Get all imported constant names across all modules
    pub fn all_imported_constant_names(&self) -> impl Iterator<Item = &String> {
        self.all_imports()
            .filter(|(_, func_id)| !self.functions.contains_key(func_id))
            .map(|(name, _)| name)
    }
}

/// Source code span (byte offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: usize, // byte offset
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
#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
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

impl std::fmt::Display for HirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for HirError {}


/// Maps imported symbol names to their function IDs.
/// Used for compile-time resolution of imports.
pub type ImportTable = HashMap<String, u32>; // symbol_name -> function_id

/// Represents a module that can be imported from.
/// A module is a collection of functions, constants, and structs identified by dot-separated paths.
#[derive(Debug, Clone, Serialize)]
pub struct Module {
    /// Functions in this module: function_name -> function_id
    pub functions: HashMap<String, u32>,
    /// Constants in this module: constant_name -> constant_id
    pub constants: HashMap<String, u32>,
    /// Structs in this module: struct_name -> StructDef
    pub structs: HashMap<String, StructDef>,
}

/// Compiler state containing all semantic information from compilation.
///
/// This is the single source of truth for the compiler's understanding of the program.
/// The LSP consumes this state instead of re-parsing or re-analyzing.
#[derive(Debug, Clone, Serialize)]
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
    pub fn new(
        ast: crate::core::ast::Program,
        hir: HirAst,
        diagnostics: Vec<HirError>,
        source: Option<&str>,
    ) -> Self {
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
    fn build_symbol_table(
        hir: &HirAst,
        ast: &crate::core::ast::Program,
        source: &str,
    ) -> SymbolTable {
        symbols::build_symbol_table(hir, ast, source)
    }

    fn build_symbol_table_without_spans(hir: &HirAst) -> SymbolTable {
        symbols::build_symbol_table_without_spans(hir)
    }
}

// HirBuilder is already re-exported above
