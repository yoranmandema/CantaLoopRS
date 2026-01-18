//! LSP-safe compiler API.
//!
//! This module exposes query-only interfaces to compiler state.
//! No tower-lsp types, no async, no editor assumptions.
//!
//! This makes the compiler usable by:
//! - LSP
//! - CLI tooling
//! - Future IDEs
//! - Tests

use std::collections::HashMap;

use crate::core::cst::{CstProgram, CstId, Span as CstSpan};
use crate::core::hir_lowering::{HirAst, HirError, SymbolId, Span as HirSpan};
use crate::core::source_manager::FileId;

/// Compiler state snapshot for LSP queries.
/// 
/// This is a **read-only, immutable** view of compilation results.
/// It represents the stable contract between the compiler and any IDE backend.
/// 
/// **Stability Boundary**: This struct defines what any future IDE backend
/// (LSP, DAP, web IDE, CLI tools) is allowed to know about CantaLoop.
/// Internal compiler structures should not leak beyond this boundary.
/// 
/// The snapshot contains:
/// - CST (Concrete Syntax Tree) by file
/// - HIR (High-level IR) if compilation succeeded
/// - Diagnostics (errors and warnings) by file
/// - Symbol table (span-to-symbol mappings)
/// - Phase 3: CST identity mappings (CstId → SymbolId, CstId → Span)
#[derive(Debug, Clone)]
pub struct CompilerSnapshot {
    /// CST by file (private - access via `cst()` method)
    csts: HashMap<FileId, CstProgram>,
    /// HIR (may be partial for multi-file projects) (private - access via `hir()` method)
    hir: Option<HirAst>,
    /// Diagnostics by file (private - access via `diagnostics()` method)
    diagnostics: HashMap<FileId, Vec<HirError>>,
    /// Symbol table (if available) (private - access via symbol query methods)
    symbols: Option<SymbolTable>,
    /// Phase 3: Mapping from CST node IDs to symbol IDs.
    /// This enables identity-based symbol resolution: Span → CstId → SymbolId
    /// NOTE: This is deprecated - symbols are now built from SymbolResolver
    cst_id_to_symbol_id: HashMap<CstId, SymbolId>,
    /// Phase 3: Mapping from CST node IDs to their source spans.
    /// This enables fast span lookup after CST is dropped.
    cst_id_to_span: HashMap<CstId, CstSpan>,
    /// File → module name mapping (for multi-file projects)
    file_to_module: HashMap<FileId, String>,
}

/// Symbol table with span information for LSP queries.
/// 
/// Internal structure - access via `CompilerSnapshot` methods.
/// Fields are public for testing.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    /// Mapping from spans to symbols
    pub span_to_symbol: HashMap<HirSpan, SymbolId>,
    /// Mapping from symbols to defining spans
    pub symbol_to_definition: HashMap<SymbolId, HirSpan>,
    /// Mapping from symbols to all reference spans (including definition)
    pub symbol_to_references: HashMap<SymbolId, Vec<HirSpan>>,
    /// Mapping from symbols to symbol info (name, kind, type)
    pub symbol_info: HashMap<SymbolId, SymbolInfo>,
}

/// Symbol stability classification.
///
/// Determines whether a symbol can be safely referenced, renamed, or modified
/// by IDE features. This is critical for rename, references, and code actions.
///
/// # Stability Levels
///
/// ## UserDefined
/// Symbols written by the user in source code. These are stable, reliable,
/// and safe for all IDE operations including rename and refactoring.
///
/// ## CompilerGenerated
/// Symbols created by the compiler (e.g., desugared constructs, closure captures).
/// These are stable within a compilation but may change representation.
/// Safe for read-only operations (hover, debugging) but not for modification.
///
/// ## Unstable
/// Symbols that may appear or disappear between snapshots. Often from
/// incomplete compilations or error recovery. Only safe for best-effort hover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolStability {
    /// User-defined symbol (function, variable, module, struct).
    ///
    /// **Safe for**:
    /// - Rename operations
    /// - Find all references
    /// - Code actions
    /// - Refactoring
    ///
    /// These symbols persist across snapshots and are the primary targets
    /// for user-facing IDE features.
    UserDefined,

    /// Compiler-generated symbol (desugared constructs, temporaries, closures).
    ///
    /// **Safe for**:
    /// - Hover information (read-only)
    /// - Debugging
    ///
    /// **NOT safe for**:
    /// - Rename (user cannot rename compiler internals)
    /// - Find references (may be misleading)
    /// - Code actions (may break on next compilation)
    ///
    /// These symbols are derived from user code but are compiler artifacts.
    /// They may change representation between compilations.
    CompilerGenerated,

    /// Unstable symbol (may disappear between snapshots).
    ///
    /// **Safe for**:
    /// - Hover information (best-effort)
    ///
    /// **NOT safe for**:
    /// - Any modification operations
    /// - Reliable reference finding
    ///
    /// These symbols exist in some snapshots but not others. They may be
    /// from incomplete compilations, error recovery, or intermediate states.
    Unstable,
}

/// Information about a symbol for LSP queries.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: crate::core::hir_lowering::SymbolKind,
    pub ty: crate::core::hir_lowering::ValueKind,
    /// Entity ID of the underlying compiler entity (for direct lookup).
    pub entity_id: Option<crate::core::EntityId>,
    /// Stability classification for this symbol.
    ///
    /// Determines which IDE features can safely operate on this symbol.
    pub stability: SymbolStability,
}

impl CompilerSnapshot {
    /// Create a new compiler snapshot.
    /// 
    /// This constructor is intentionally only accessible within the `core` module.
    /// Snapshots should be created by `CompilerState` and consumed by query APIs.
    pub(crate) fn new(
        csts: HashMap<FileId, CstProgram>,
        hir: Option<HirAst>,
        diagnostics: HashMap<FileId, Vec<HirError>>,
        symbols: Option<SymbolTable>,
        cst_id_to_span: HashMap<CstId, CstSpan>,
        file_to_module: HashMap<FileId, String>,
    ) -> Self {
        Self {
            csts,
            hir,
            diagnostics,
            symbols,
            cst_id_to_symbol_id: HashMap::new(), // No longer used - symbols are built from SymbolResolver
            cst_id_to_span,
            file_to_module,
        }
    }

    /// Get CST for a file.
    pub fn cst(&self, file_id: FileId) -> Option<&CstProgram> {
        self.csts.get(&file_id)
    }

    /// Get HIR (may be None if compilation failed).
    pub fn hir(&self) -> Option<&HirAst> {
        self.hir.as_ref()
    }

    /// Get diagnostics for a file.
    /// 
    /// Returns an empty slice if the file has no diagnostics.
    pub fn diagnostics(&self, file_id: FileId) -> &[HirError] {
        self.diagnostics.get(&file_id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get all file IDs that have CSTs in this snapshot.
    pub fn file_ids(&self) -> impl Iterator<Item = FileId> + '_ {
        self.csts.keys().copied()
    }

    /// Check if a file has diagnostics.
    pub fn has_diagnostics(&self, file_id: FileId) -> bool {
        self.diagnostics.get(&file_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Get symbol at a span.
    pub fn symbol_at(&self, file_id: FileId, span: HirSpan) -> Option<SymbolId> {
        self.symbols
            .as_ref()?
            .span_to_symbol
            .get(&span)
            .copied()
    }

    /// Get defining span(s) for a symbol.
    pub fn spans_for_symbol(&self, symbol_id: SymbolId) -> Option<&[HirSpan]> {
        self.symbols
            .as_ref()?
            .symbol_to_references
            .get(&symbol_id)
            .map(|v| v.as_slice())
    }

    /// Get definition span for a symbol.
    pub fn definition_span_for_symbol(&self, symbol_id: SymbolId) -> Option<HirSpan> {
        self.symbols
            .as_ref()?
            .symbol_to_definition
            .get(&symbol_id)
            .copied()
    }

    /// Get symbol information.
    pub fn symbol_info(&self, symbol_id: SymbolId) -> Option<&SymbolInfo> {
        self.symbols
            .as_ref()?
            .symbol_info
            .get(&symbol_id)
    }

    /// Check if a symbol is stable enough for rename operations.
    ///
    /// Only `UserDefined` symbols can be safely renamed.
    pub fn can_rename(&self, symbol_id: SymbolId) -> bool {
        self.symbol_info(symbol_id)
            .map(|info| matches!(info.stability, SymbolStability::UserDefined))
            .unwrap_or(false)
    }

    /// Check if a symbol is stable enough for reliable reference finding.
    ///
    /// Both `UserDefined` and `CompilerGenerated` symbols can have references found,
    /// but `Unstable` symbols may not have complete reference information.
    pub fn has_reliable_references(&self, symbol_id: SymbolId) -> bool {
        self.symbol_info(symbol_id)
            .map(|info| {
                matches!(
                    info.stability,
                    SymbolStability::UserDefined | SymbolStability::CompilerGenerated
                )
            })
            .unwrap_or(false)
    }

    /// Find all symbols that have spans containing the given byte offset.
    /// 
    /// Uses span-based lookup from the symbol table, which includes both definitions and references.
    /// 
    /// Returns an iterator over (span, symbol_id) pairs where the span contains the offset.
    /// The iterator is ordered by span size (smallest first) to prefer more specific matches.
    pub fn symbols_at_offset(&self, file_id: FileId, byte_offset: usize) -> impl Iterator<Item = (HirSpan, SymbolId)> + '_ {
        let mut matches: Vec<(HirSpan, SymbolId)> = Vec::new();

        eprintln!("[LSP_API] symbols_at_offset: Looking for symbols at byte_offset={}", byte_offset);

        // Use span-based lookup from symbol table (includes both definitions and references)
        // CRITICAL: Spans are half-open intervals [start, end), so end is exclusive
        if let Some(symbols) = self.symbols.as_ref() {
            eprintln!("[LSP_API] symbols_at_offset: Checking {} span->symbol mappings", symbols.span_to_symbol.len());
            for (span, symbol_id) in symbols.span_to_symbol.iter().take(5) {
                if let Some(info) = symbols.symbol_info.get(symbol_id) {
                    eprintln!("[LSP_API]   Sample span: {}..{} -> {} (symbol_id={:?})",
                        span.start, span.end, info.name, symbol_id);
                }
            }

            matches = symbols.span_to_symbol
                .iter()
                .filter_map(|(span, &symbol_id)| {
                    // Half-open interval: [start, end) - end is exclusive
                    if span.start <= byte_offset && byte_offset < span.end {
                        eprintln!("[LSP_API] symbols_at_offset: MATCH! span {}..{} contains offset {}",
                            span.start, span.end, byte_offset);
                        Some((*span, symbol_id))
                    } else {
                        None
                    }
                })
                .collect();
        }
        
        // Sort by span length (smallest first) to prefer more specific matches
        // This ensures that when hovering over "x" in "let x = y", we get the "x" identifier
        // rather than the larger "let x = y" expression span
        matches.sort_by_key(|(span, _)| span.end - span.start);
        
        matches.into_iter()
    }
    
    /// Find CST node IDs at a given byte offset.
    /// Returns all CST nodes whose spans contain the offset, ordered by specificity (smallest first).
    fn find_cst_nodes_at_offset(cst: &CstProgram, byte_offset: usize) -> Vec<CstId> {
        use crate::core::cst::{CstExpr, CstStatement};
        
        let mut nodes = Vec::new();
        
        fn walk_expr(expr: &crate::core::cst::Spanned<CstExpr>, offset: usize, nodes: &mut Vec<CstId>) {
            let span = expr.span;
            // CRITICAL: Spans are half-open intervals [start, end), so end is exclusive
            if span.start as usize <= offset && offset < span.end as usize {
                nodes.push(expr.id);
                
                // Recurse into children for more specific matches
                match &expr.node {
                    CstExpr::Identifier(_) | CstExpr::Literal(_) => {
                        // Leaf nodes - already added
                    }
                    CstExpr::Infix { lhs, rhs, .. } => {
                        walk_expr(lhs, offset, nodes);
                        walk_expr(rhs, offset, nodes);
                    }
                    CstExpr::Prefix { rhs, .. } => {
                        walk_expr(rhs, offset, nodes);
                    }
                    CstExpr::Postfix { lhs, .. } => {
                        walk_expr(lhs, offset, nodes);
                    }
                    CstExpr::FunctionCall { callee, arguments, .. } => {
                        walk_expr(callee, offset, nodes);
                        // Arguments are CstCallArgument, not CstExpr - skip for now
                        // TODO: Extract expressions from CstCallArgument if needed
                    }
                    CstExpr::PartialCall { func, args, .. } => {
                        walk_expr(func, offset, nodes);
                        // Arguments are CstCallArgument, not CstExpr - skip for now
                    }
                    CstExpr::MemberAccess { object, .. } => {
                        walk_expr(object, offset, nodes);
                    }
                    _ => {
                        // Other expression types - could add more specific handling
                    }
                }
            }
        }
        
        // Walk through all blocks and statements
        for block in &cst.blocks {
            let block_span = block.span;
            // CRITICAL: Spans are half-open intervals [start, end), so end is exclusive
            if block_span.start as usize <= byte_offset && byte_offset < block_span.end as usize {
                for stmt in &block.node.statements {
                    let stmt_span = stmt.span;
                    if stmt_span.start as usize <= byte_offset && byte_offset < stmt_span.end as usize {
                        match &stmt.node {
                            CstStatement::Let { identifier, expression, .. } => {
                                if identifier.span.start as usize <= byte_offset && byte_offset < identifier.span.end as usize {
                                    nodes.push(identifier.id);
                                }
                                walk_expr(expression, byte_offset, &mut nodes);
                            }
                            CstStatement::Const { identifier, expression, .. } => {
                                if identifier.span.start as usize <= byte_offset && byte_offset < identifier.span.end as usize {
                                    nodes.push(identifier.id);
                                }
                                walk_expr(expression, byte_offset, &mut nodes);
                            }
                            CstStatement::FunctionDeclaration { identifier, .. } => {
                                if identifier.span.start as usize <= byte_offset && byte_offset < identifier.span.end as usize {
                                    nodes.push(identifier.id);
                                }
                            }
                            CstStatement::Expression(expr) => {
                                walk_expr(expr, byte_offset, &mut nodes);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        
        // Sort by span size (smallest first) - we'll need to look up spans
        // For now, just return all matches
        nodes
    }

    /// Check if symbol table is available.
    pub fn has_symbols(&self) -> bool {
        self.symbols.is_some()
    }

    /// Get symbol table for iteration (if available).
    /// 
    /// This is only for cases where full iteration is needed (e.g., semantic tokens).
    /// Prefer the specific query methods when possible.
    pub fn symbol_table(&self) -> Option<&SymbolTable> {
        self.symbols.as_ref()
    }
}

/// Query API for compiler state.
/// 
/// This trait allows the LSP to query compiler state without
/// depending on internal compiler structures.
pub trait CompilerQueryApi {
    /// Parse a file and return CST.
    fn parse(&self, file_id: FileId) -> Option<&CstProgram>;
    
    /// Get HIR for the project.
    fn hir(&self) -> Option<&HirAst>;
    
    /// Get diagnostics for a file.
    fn diagnostics(&self, file_id: FileId) -> &[HirError];
    
    /// Get symbol at a span.
    fn symbol_at(&self, file_id: FileId, span: HirSpan) -> Option<SymbolId>;
    
    /// Get all spans (references + definition) for a symbol.
    fn spans_for_symbol(&self, symbol_id: SymbolId) -> Option<&[HirSpan]>;
    
    /// Get definition span for a symbol.
    fn definition_span_for_symbol(&self, symbol_id: SymbolId) -> Option<HirSpan>;
}

impl CompilerQueryApi for CompilerSnapshot {
    fn parse(&self, file_id: FileId) -> Option<&CstProgram> {
        self.cst(file_id)
    }

    fn hir(&self) -> Option<&HirAst> {
        self.hir()
    }

    fn diagnostics(&self, file_id: FileId) -> &[HirError] {
        self.diagnostics(file_id)
    }

    fn symbol_at(&self, file_id: FileId, span: HirSpan) -> Option<SymbolId> {
        self.symbol_at(file_id, span)
    }

    fn spans_for_symbol(&self, symbol_id: SymbolId) -> Option<&[HirSpan]> {
        self.spans_for_symbol(symbol_id)
    }

    fn definition_span_for_symbol(&self, symbol_id: SymbolId) -> Option<HirSpan> {
        self.definition_span_for_symbol(symbol_id)
    }
}
