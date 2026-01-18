//! Identity-first symbol resolution for LSP.
//!
//! This module provides a clean architecture for tracking symbol definitions and references
//! using CST identity (CstId) and entity identity (EntityId). This eliminates the need for
//! fragile name-based matching and unsafe pointer mechanisms.
//!
//! Architecture:
//! ```text
//! CST Node (with CstId)
//!     ↓ during HIR lowering
//! HIR resolves identifier → EntityId
//!     ↓ record mapping
//! CstId → EntityId
//!     ↓ during symbol building
//! Create Symbol with entity_id and CST span
//!     ↓
//! span_to_symbol populated from CstId→EntityId map
//! ```

use std::collections::HashMap;
use crate::core::cst::CstId;
use crate::core::entity_id::EntityId;
use crate::core::hir_lowering::{SymbolKind, ValueKind, Span as HirSpan};
use crate::core::cst::Span as CstSpan;
use crate::core::source_manager::FileId;

/// Metadata about a symbol entity for LSP queries.
#[derive(Debug, Clone)]
pub struct SymbolMetadata {
    /// Human-readable name (for display and debugging).
    pub name: String,
    /// Kind of symbol (function, variable, parameter, module).
    pub kind: SymbolKind,
    /// Type of this symbol.
    pub value_kind: ValueKind,
    /// CST node ID where this symbol is defined (if available).
    pub definition_cst_id: Option<CstId>,
    /// CST node IDs where this symbol is referenced.
    pub reference_cst_ids: Vec<CstId>,
}

impl SymbolMetadata {
    pub fn new(name: String, kind: SymbolKind, value_kind: ValueKind) -> Self {
        Self {
            name,
            kind,
            value_kind,
            definition_cst_id: None,
            reference_cst_ids: Vec::new(),
        }
    }
}

/// Identity-first symbol resolver.
///
/// Tracks the mapping from CST nodes (CstId) to resolved entities (EntityId)
/// during HIR lowering. This provides a clean, safe way to build symbol tables
/// without name-based matching or unsafe pointer mechanisms.
pub struct SymbolResolver {
    /// Direct mapping from CST node identity to resolved entity.
    cst_to_entity: HashMap<CstId, EntityId>,

    /// Track entity metadata for symbol building.
    entity_metadata: HashMap<EntityId, SymbolMetadata>,
}

impl SymbolResolver {
    /// Create a new symbol resolver.
    pub fn new() -> Self {
        Self {
            cst_to_entity: HashMap::new(),
            entity_metadata: HashMap::new(),
        }
    }

    /// Record a symbol definition.
    ///
    /// This should be called when processing declarations (function declarations,
    /// variable initializations, parameter declarations, etc.).
    pub fn define(&mut self, cst_id: CstId, entity_id: EntityId, metadata: SymbolMetadata) {
        self.cst_to_entity.insert(cst_id, entity_id);
        
        // Update or create metadata
        let mut meta = metadata;
        meta.definition_cst_id = Some(cst_id);
        self.entity_metadata.insert(entity_id, meta);
    }

    /// Record a symbol reference.
    ///
    /// This should be called when resolving identifiers (variable lookups,
    /// function calls, etc.).
    pub fn reference(&mut self, cst_id: CstId, entity_id: EntityId) {
        self.cst_to_entity.insert(cst_id, entity_id);

        // Add to reference list if entity exists
        if let Some(meta) = self.entity_metadata.get_mut(&entity_id) {
            meta.reference_cst_ids.push(cst_id);
        }
    }

    /// Get the entity ID for a CST node (if resolved).
    pub fn get_entity(&self, cst_id: CstId) -> Option<EntityId> {
        self.cst_to_entity.get(&cst_id).copied()
    }

    /// Get metadata for an entity.
    pub fn get_metadata(&self, entity_id: EntityId) -> Option<&SymbolMetadata> {
        self.entity_metadata.get(&entity_id)
    }

    /// Update the value kind for an existing entity.
    ///
    /// This is used when we refine types after initial lowering (e.g., inferring reducer closure parameter types).
    pub fn update_value_kind(&mut self, entity_id: EntityId, value_kind: ValueKind) {
        if let Some(meta) = self.entity_metadata.get_mut(&entity_id) {
            meta.value_kind = value_kind;
        }
    }

    /// Get all CST-to-entity mappings.
    pub fn cst_to_entity_map(&self) -> &HashMap<CstId, EntityId> {
        &self.cst_to_entity
    }

    /// Get all entity metadata.
    pub fn entity_metadata_map(&self) -> &HashMap<EntityId, SymbolMetadata> {
        &self.entity_metadata
    }

    /// Build final symbol table from resolver state.
    ///
    /// This converts the identity-based mappings into span-based lookups
    /// for LSP queries. The resulting SymbolTable can be used by LSP handlers
    /// for hover, go-to-definition, and semantic tokens.
    pub fn build_symbol_table(
        &self,
        cst_id_to_span: &HashMap<CstId, CstSpan>,
        cst_id_to_file: &HashMap<CstId, FileId>,
    ) -> crate::core::lsp_api::SymbolTable {
        use crate::core::lsp_api::{SymbolTable, SymbolInfo, SymbolStability};
        use crate::core::hir_lowering::SymbolId;

        let mut span_to_symbol = HashMap::new();
        let mut symbol_to_definition = HashMap::new();
        let mut symbol_to_references = HashMap::new();
        let mut symbol_info = HashMap::new();

        // Convert EntityId to SymbolId (they're the same now, but we need SymbolId for the table)
        let entity_to_symbol_id = |entity_id: EntityId| -> SymbolId {
            SymbolId(entity_id.as_u32())
        };

        // Build symbol table from entity metadata
        for (entity_id, metadata) in &self.entity_metadata {
            let symbol_id = entity_to_symbol_id(*entity_id);

            // Create symbol info
            let info = SymbolInfo {
                name: metadata.name.clone(),
                kind: metadata.kind.clone(),
                ty: metadata.value_kind.clone(),
                entity_id: Some(*entity_id),
                stability: SymbolStability::UserDefined, // All symbols from HIR are user-defined
            };
            symbol_info.insert(symbol_id, info);

            // Add definition span if available
            if let Some(def_cst_id) = metadata.definition_cst_id {
                if let Some(&cst_span) = cst_id_to_span.get(&def_cst_id) {
                    let hir_span = HirSpan::new(cst_span.start as usize, cst_span.end as usize);
                    symbol_to_definition.insert(symbol_id, hir_span);
                    span_to_symbol.insert(hir_span, symbol_id);
                    
                    // Initialize references list with definition
                    symbol_to_references.entry(symbol_id)
                        .or_insert_with(Vec::new)
                        .push(hir_span);
                }
            }

            // Add reference spans
            for ref_cst_id in &metadata.reference_cst_ids {
                if let Some(&cst_span) = cst_id_to_span.get(ref_cst_id) {
                    let hir_span = HirSpan::new(cst_span.start as usize, cst_span.end as usize);
                    span_to_symbol.insert(hir_span, symbol_id);

                    // Add to references list
                    symbol_to_references.entry(symbol_id)
                        .or_insert_with(Vec::new)
                        .push(hir_span);
                }
            }
        }

        // CRITICAL: Also map all CST nodes that were resolved (cst_to_entity map)
        // This ensures ALL references are included, even if they were recorded before the entity had metadata
        for (cst_id, entity_id) in &self.cst_to_entity {
                if let Some(&cst_span) = cst_id_to_span.get(cst_id) {
                    let hir_span = HirSpan::new(cst_span.start as usize, cst_span.end as usize);
                    let symbol_id = entity_to_symbol_id(*entity_id);
                    
                // Check if this is a definition (already handled above)
                let is_definition = self.entity_metadata.get(entity_id)
                    .and_then(|meta| meta.definition_cst_id)
                    .map(|def_id| def_id == *cst_id)
                    .unwrap_or(false);
                
                if !is_definition {
                    // This is a reference (or an entity without metadata yet)
                    // Only add if not already in span_to_symbol (avoid overwriting definitions)
                    if !span_to_symbol.contains_key(&hir_span) {
                        // If entity has metadata, use it; otherwise create minimal entry
                    if !symbol_info.contains_key(&symbol_id) {
                            if let Some(meta) = self.entity_metadata.get(entity_id) {
                                // Entity has metadata but this CST ID wasn't in reference_cst_ids
                                // This can happen if reference was recorded differently
                                let info = SymbolInfo {
                                    name: meta.name.clone(),
                                    kind: meta.kind.clone(),
                                    ty: meta.value_kind.clone(),
                                    entity_id: Some(*entity_id),
                                    stability: SymbolStability::UserDefined,
                                };
                                symbol_info.insert(symbol_id, info);
                            } else {
                                // No metadata - create minimal entry
                                let info = SymbolInfo {
                                    name: format!("<unknown:{}>", entity_id.as_u32()),
                                    kind: SymbolKind::Variable,
                                    ty: ValueKind::Unknown,
                                    entity_id: Some(*entity_id),
                                    stability: SymbolStability::Unstable,
                                };
                        symbol_info.insert(symbol_id, info);
                            }
                    }
                    
                    span_to_symbol.insert(hir_span, symbol_id);
                    symbol_to_references.entry(symbol_id)
                        .or_insert_with(Vec::new)
                        .push(hir_span);
                    }
                }
            }
        }

        SymbolTable {
            span_to_symbol,
            symbol_to_definition,
            symbol_to_references,
            symbol_info,
        }
    }
}

impl Default for SymbolResolver {
    fn default() -> Self {
        Self::new()
    }
}
