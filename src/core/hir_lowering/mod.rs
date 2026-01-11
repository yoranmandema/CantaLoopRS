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
pub mod symbol_occurrences;

// Re-export core types
pub use lower_expr::{HirExpression, ReducerType};
pub use lower_stmt::{HirBlock, HirBuilder, HirStmt};
pub use project_semantic_items::{SemanticItem, SemanticItemKind, SemanticModifiers};
pub use scopes::{HirBlockContext, Scope, ScopeArena, ScopeId, ScopeIdOld};
use serde::Serialize;
pub use symbols::{Symbol, SymbolId, SymbolKind, SymbolTable, TypeId};
pub use symbol_occurrences::{SymbolOccurrence, SymbolOccurrences, SymbolRole};

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
    /// Whether this function is effectful (uses ~>) or pure (uses ->)
    pub is_effectful: bool,
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
    // Callable type: represents anything that can be called (function, thunk, or closure)
    // Used in native function signatures to accept callable arguments
    Callable,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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
    NotImplemented {
        span: Span,
    },
    UnknownVariable {
        name: String,
        span: Span,
    },
    VariableAlreadyDeclared {
        name: String,
        span: Span,
    },
    TypeMismatch {
        variable: String,
        expected: ValueKind,
        actual: ValueKind,
        span: Span,
    },
    TypeError {
        message: String,
        span: Span,
    },
    BinaryOpTypeError {
        operator: String,
        lhs_type: ValueKind,
        rhs_type: ValueKind,
        expected: String,
        span: Span,
    },
    MemberNotFound {
        member: String,
        object_type: String,
        span: Span,
    },
    FunctionNotFound {
        name: String,
        span: Span,
    },
    ModuleNotFound {
        module_path: String,
        span: Span,
    },
    // You can add more specific error variants as needed
}

impl std::fmt::Display for HirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HirError::NotImplemented { .. } => write!(f, "Not implemented"),
            HirError::UnknownVariable { name, .. } => {
                write!(f, "Variable '{}' is not declared. Use 'let' to declare a new variable.", name)
            }
            HirError::VariableAlreadyDeclared { name, .. } => {
                write!(f, "Variable '{}' is already declared", name)
            }
            HirError::TypeMismatch { variable, expected, actual, .. } => {
                write!(f, "Type mismatch for variable '{}': expected {:?}, got {:?}", variable, expected, actual)
            }
            HirError::TypeError { message, .. } => write!(f, "{}", message),
            HirError::BinaryOpTypeError { operator, lhs_type, rhs_type, expected, .. } => {
                write!(f, "{} operation requires {}, but got {:?} and {:?}", operator, expected, lhs_type, rhs_type)
            }
            HirError::MemberNotFound { member, object_type, .. } => {
                write!(f, "Member '{}' not found on type '{}'", member, object_type)
            }
            HirError::FunctionNotFound { name, .. } => {
                write!(f, "Function or constant '{}' not found", name)
            }
            HirError::ModuleNotFound { module_path, .. } => {
                write!(f, "Module '{}' not found", module_path)
            }
        }
    }
}

impl std::error::Error for HirError {}

impl HirError {
    /// Create a synthetic span (0,0) - used when span information is not available during lowering.
    /// This should be replaced with actual spans when span information is available.
    pub fn synthetic_span() -> Span {
        Span::new(0, 0)
    }
    
    /// Get the span from this error.
    pub fn span(&self) -> Span {
        match self {
            HirError::NotImplemented { span } => *span,
            HirError::UnknownVariable { span, .. } => *span,
            HirError::VariableAlreadyDeclared { span, .. } => *span,
            HirError::TypeMismatch { span, .. } => *span,
            HirError::TypeError { span, .. } => *span,
            HirError::BinaryOpTypeError { span, .. } => *span,
            HirError::MemberNotFound { span, .. } => *span,
            HirError::FunctionNotFound { span, .. } => *span,
            HirError::ModuleNotFound { span, .. } => *span,
        }
    }
}


/// Maps imported symbol names to their function IDs.
/// Used for compile-time resolution of imports.
pub type ImportTable = HashMap<String, u32>; // symbol_name -> function_id

/// Represents a module that can be imported from.
/// A module is a collection of functions, constants, and structs identified by dot-separated paths.
/// Each module has its own import scope to avoid collisions across files.
#[derive(Debug, Clone, Serialize)]
pub struct Module {
    /// Functions in this module: function_name -> function_id
    pub functions: HashMap<String, u32>,
    /// Constants in this module: constant_name -> constant_id
    pub constants: HashMap<String, u32>,
    /// Structs in this module: struct_name -> StructDef
    pub structs: HashMap<String, StructDef>,
    /// Imported symbols in this module: symbol_name -> function_id
    /// This is per-module to avoid collisions when the same symbol is imported in different files.
    pub imports: HashMap<String, u32>,
}

/// Compiler state containing all semantic information from compilation.
///
/// This is the single source of truth for the compiler's understanding of the program.
/// The LSP consumes this state instead of re-parsing or re-analyzing.
#[derive(Debug, Clone, Serialize)]
pub struct CompilerState {
    /// Concrete Syntax Tree - preserves exact source spans for LSP.
    /// Used for fast syntax highlighting (keywords, operators, literals) without semantic analysis.
    pub cst: crate::core::cst::CstProgram,
    /// Abstract Syntax Tree - semantic representation without spans.
    pub ast: crate::core::ast::Program,
    pub hir: HirAst,
    pub diagnostics: Vec<HirError>,
    pub symbols: SymbolTable,
    /// Unified semantic items (keywords, operators, identifiers, types) extracted from AST.
    /// This is the single source for all LSP semantic tokenization - no text scanning needed.
    pub semantic_items: Vec<SemanticItem>,
    /// Precomputed line index for efficient byte-to-line/column conversion.
    pub line_index: Option<LineIndex>,
    /// Documentation blocks keyed by declaration identifier name.
    pub docs: std::collections::HashMap<String, crate::core::cst::DocBlock>,
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
        cst: crate::core::cst::CstProgram,
        ast: crate::core::ast::Program,
        hir: HirAst,
        diagnostics: Vec<HirError>,
        source: Option<&str>,
        docs: std::collections::HashMap<String, crate::core::cst::DocBlock>,
    ) -> Self {
        let symbols = if let Some(_src) = source {
            Self::build_symbol_table(&hir, &ast, &cst)
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
            cst,
            ast,
            hir,
            diagnostics,
            symbols,
            semantic_items,
            line_index,
            docs,
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

    /// Query documentation for a declaration by identifier name.
    ///
    /// This is the stable internal API for tooling to access documentation.
    /// The compiler does not interpret docs - this is purely for tooling consumption.
    ///
    /// # Arguments
    /// * `identifier` - The name of the declaration (function, const, let, struct, mod)
    ///
    /// # Returns
    /// * `Some(&DocBlock)` if documentation exists for this identifier
    /// * `None` if no documentation is available
    ///
    /// # Example
    /// ```ignore
    /// if let Some(doc) = compiler_state.docs_for("my_function") {
    ///     println!("Documentation: {}", doc.text);
    /// }
    /// ```
    pub fn docs_for(&self, identifier: &str) -> Option<&crate::core::cst::DocBlock> {
        self.docs.get(identifier)
    }

    /// Get all documentation blocks in source order.
    ///
    /// Returns documentation blocks in the order they appear in the source code,
    /// matching Cantaloop's "code as execution" philosophy.
    ///
    /// This is useful for deterministic doc extraction tools.
    pub fn docs_in_source_order(&self, source: &str) -> Vec<(&String, &crate::core::cst::DocBlock)> {
        // Build a map of identifier -> (line_number, identifier, doc)
        let mut docs_with_positions: Vec<(usize, &String, &crate::core::cst::DocBlock)> = Vec::new();
        
        for (identifier, doc) in &self.docs {
            // Find the line number of the declaration by searching in AST
            let line_num = self.find_declaration_line(identifier, source);
            docs_with_positions.push((line_num, identifier, doc));
        }
        
        // Sort by line number (source order)
        docs_with_positions.sort_by_key(|(line, _, _)| *line);
        
        // Return (identifier, doc) pairs
        docs_with_positions.into_iter().map(|(_, ident, doc)| (ident, doc)).collect()
    }

    /// Find the line number of a declaration in source code.
    fn find_declaration_line(&self, identifier: &str, source: &str) -> usize {
        // Search through AST to find the declaration
        for block in &self.ast.blocks {
            for stmt in &block.statements {
                match stmt {
                    crate::core::ast::Statement::FunctionDeclaration { identifier: name, .. } 
                        if name.name == identifier => {
                        // Find in CST to get span
                        if let Some(span) = self.find_cst_span_for_declaration(identifier) {
                            if let Some(idx) = &self.line_index {
                                let (line, _) = idx.lookup(span.start as usize);
                                return line as usize;
                            }
                        }
                    }
                    crate::core::ast::Statement::Const { identifier: name, .. } 
                        if name.name == identifier => {
                        if let Some(span) = self.find_cst_span_for_declaration(identifier) {
                            if let Some(idx) = &self.line_index {
                                let (line, _) = idx.lookup(span.start as usize);
                                return line as usize;
                            }
                        }
                    }
                    crate::core::ast::Statement::Let { identifier: name, .. } 
                        if name.name == identifier => {
                        if let Some(span) = self.find_cst_span_for_declaration(identifier) {
                            if let Some(idx) = &self.line_index {
                                let (line, _) = idx.lookup(span.start as usize);
                                return line as usize;
                            }
                        }
                    }
                    crate::core::ast::Statement::Struct { name, .. } 
                        if name == identifier => {
                        if let Some(span) = self.find_cst_span_for_declaration(identifier) {
                            if let Some(idx) = &self.line_index {
                                let (line, _) = idx.lookup(span.start as usize);
                                return line as usize;
                            }
                        }
                    }
                    crate::core::ast::Statement::Mod { identifier: name } 
                        if name == identifier => {
                        if let Some(span) = self.find_cst_span_for_declaration(identifier) {
                            if let Some(idx) = &self.line_index {
                                let (line, _) = idx.lookup(span.start as usize);
                                return line as usize;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        
        // Fallback: search in source text
        for (line_num, line) in source.lines().enumerate() {
            if line.contains(&format!("{}", identifier)) {
                return line_num;
            }
        }
        
        0
    }

    /// Find the CST span for a declaration identifier.
    pub fn find_cst_span_for_declaration(&self, identifier: &str) -> Option<crate::core::cst::Span> {
        // Search through CST to find the declaration
        for block in &self.cst.blocks {
            for stmt in &block.node.statements {
                match &stmt.node {
                    crate::core::cst::CstStatement::FunctionDeclaration { identifier: name, .. } 
                        if name.node == identifier => {
                        return Some(name.span);
                    }
                    crate::core::cst::CstStatement::Const { identifier: name, .. } 
                        if name.node == identifier => {
                        return Some(name.span);
                    }
                    crate::core::cst::CstStatement::Let { identifier: name, .. } 
                        if name.node == identifier => {
                        return Some(name.span);
                    }
                    crate::core::cst::CstStatement::Struct { name, .. } 
                        if name.node == identifier => {
                        return Some(name.span);
                    }
                    crate::core::cst::CstStatement::Mod { identifier: name } 
                        if name.node == identifier => {
                        return Some(name.span);
                    }
                    _ => {}
                }
            }
        }
        None
    }

    // Symbol table building functions (moved from symbols module for now due to dependencies)
    fn build_symbol_table(
        hir: &HirAst,
        ast: &crate::core::ast::Program,
        cst: &crate::core::cst::CstProgram,
    ) -> SymbolTable {
        symbols::build_symbol_table(hir, ast, cst)
    }

    fn build_symbol_table_without_spans(hir: &HirAst) -> SymbolTable {
        symbols::build_symbol_table_without_spans(hir)
    }
}

// HirBuilder is already re-exported above
