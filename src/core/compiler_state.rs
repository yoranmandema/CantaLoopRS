//! Compiler state manager for LSP.
//!
//! Manages incremental compilation and provides query API for LSP.
//! This bridges CompileSession (single-use) with LSP's persistent state needs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::{Path, PathBuf};
use arc_swap::ArcSwap;

use crate::core::cst::{parse_cst_program, lower_cst_to_ast, CstProgram, CstExpr, CstStatement, Spanned};
use crate::core::hir_lowering::{HirBuilder, HirAst, Span as HirSpan};
use crate::core::ast::Program;
use crate::core::compileSession::CompileContext;
use crate::core::engine::Engine;
use crate::core::lsp_api::{CompilerSnapshot, SymbolTable};
use crate::core::source_manager::{FileId, SourceManager};
use tower_lsp::lsp_types::Url;
use crate::core::hir_lowering::symbols::build_symbol_table;
use crate::stdlib;

// File logger for debugging LSP issues
fn log_to_file(msg: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("lsp_debug.log")
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let _ = writeln!(file, "[{}] {}", timestamp, msg);
    }
}

/// Compiler state manager.
///
/// Manages compilation state across file changes and provides
/// query API for LSP features.
pub struct CompilerState {
    source_manager: Arc<RwLock<SourceManager>>,
    /// Current compiler snapshot (read-only view) - only updated on successful compilation
    /// CRITICAL: Uses ArcSwap for lock-free reads - LSP handlers can read without blocking compilation
    snapshot: ArcSwap<Option<Arc<CompilerSnapshot>>>,
    /// Last good snapshot per file - preserved even when current file fails to parse
    /// Key: FileId, Value: Last known good snapshot
    /// Note: This still uses RwLock because we need to update per-file snapshots atomically
    last_good_snapshots: Arc<RwLock<HashMap<FileId, CompilerSnapshot>>>,
    /// Engine with stdlib loaded (for native functions)
    engine: Arc<Engine>,
    /// Root files that were opened via didOpen (editor-driven entry points)
    open_roots: Arc<RwLock<HashSet<FileId>>>,
    /// Workspace folders from last initialize - used to detect workspace changes
    workspace_folders: Arc<RwLock<Option<Vec<String>>>>,
}

impl CompilerState {
    pub fn new(source_manager: Arc<RwLock<SourceManager>>) -> Self {
        // Initialize engine with stdlib
        let mut engine = Engine::new();
        stdlib::load_stdlib_runtime(&mut engine);
        
        Self {
            source_manager,
            snapshot: ArcSwap::from_pointee(None),
            last_good_snapshots: Arc::new(RwLock::new(HashMap::new())),
            engine: Arc::new(engine),
            open_roots: Arc::new(RwLock::new(HashSet::new())),
            workspace_folders: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Mark a file as an open root (called from didOpen).
    pub async fn mark_as_root(&self, file_id: FileId) -> Result<(), String> {
        let mut roots = self.open_roots.write().await;
        roots.insert(file_id);
        log::debug!("Marked file {} as root", file_id.0);
        Ok(())
    }
    
    /// Clear all caches and state (called on initialize/reconnect).
    /// This ensures a fresh start when VSCode reconnects without restarting the process.
    pub async fn clear_all_caches(&self) {
        eprintln!("[LSP] Cleared all caches on initialize");
        log::info!("Cleared all caches on initialize");
        
        // Clear snapshots (lock-free write)
        // ArcSwap stores Arc<T>, so we wrap None in Arc
        self.snapshot.store(Arc::new(None));
        *self.last_good_snapshots.write().await = HashMap::new();
        
        // Clear open roots
        *self.open_roots.write().await = HashSet::new();
        
        // Note: We don't clear source_manager here because files might still be open
        // The source_manager will be updated as files are opened/closed
    }
    
    /// Update workspace folders and clear caches if they changed.
    /// Returns true if workspace folders changed (and caches were cleared).
    pub async fn update_workspace_folders(&self, new_folders: Option<Vec<String>>) -> bool {
        let mut current_folders = self.workspace_folders.write().await;
        
        // Normalize folder paths for comparison (convert to canonical paths if possible)
        let normalized_new = new_folders.as_ref().map(|folders| {
            folders.iter().map(|f| {
                // Try to canonicalize, but fall back to original if it fails
                std::path::Path::new(f)
                    .canonicalize()
                    .ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| f.clone())
            }).collect::<Vec<_>>()
        });
        
        let normalized_current = current_folders.as_ref().map(|folders| {
            folders.iter().map(|f| {
                std::path::Path::new(f)
                    .canonicalize()
                    .ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| f.clone())
            }).collect::<Vec<_>>()
        });
        
        // Check if workspace folders changed
        let folders_changed = normalized_new != normalized_current;
        
        if folders_changed {
            eprintln!("[LSP] Workspace folders changed - clearing all caches");
            log::warn!("Workspace folders changed - clearing all caches");
            
            // Clear everything
            self.clear_all_caches().await;
        }
        
        // Update stored workspace folders
        *current_folders = new_folders;
        
        folders_changed
    }
    
    /// Find project root by walking up from a file path.
    /// Stops at `.git`, `melon.json`, or workspace root.
    fn find_project_root(file_path: &Path) -> Option<PathBuf> {
        let mut current = file_path.parent()?;
        
        loop {
            // Check for project markers
            if current.join(".git").exists() || current.join("melon.json").exists() {
                return Some(current.to_path_buf());
            }
            
            // Stop at filesystem root
            if let Some(parent) = current.parent() {
                current = parent;
            } else {
                return Some(current.to_path_buf());
            }
        }
    }
    
    /// Resolve module file path from a mod statement.
    /// `mod utils;` → `src/utils.cl` or `utils.cl` (in same directory as current file)
    fn resolve_module_path(module_name: &str, current_file_path: &Path, project_root: Option<&Path>) -> Option<PathBuf> {
        let current_dir = current_file_path.parent()?;
        
        // Try same directory first: current_dir/utils.cl
        let same_dir = current_dir.join(format!("{}.cl", module_name));
        if same_dir.exists() {
            return Some(same_dir);
        }
        
        // Try src/ directory if project root exists
        if let Some(root) = project_root {
            let src_path = root.join("src").join(format!("{}.cl", module_name));
            if src_path.exists() {
                return Some(src_path);
            }
        }
        
        None
    }
    
    /// Discover and load modules referenced by mod statements in a file.
    /// Returns list of loaded FileIds.
    /// This is a sync helper to avoid async recursion issues.
    fn discover_and_load_modules_sync(
        &self,
        file_id: FileId,
        cst: &CstProgram,
        loaded_modules: &mut HashSet<FileId>,
        source_manager: &SourceManager,
    ) -> Result<Vec<FileId>, String> {
        // Get current file path
        let current_uri = source_manager.get_uri(file_id).cloned();
        
        let current_path = match current_uri {
            Some(uri) => uri.to_file_path().map_err(|_| "URI is not a file path".to_string())?,
            None => return Ok(vec![]),
        };
        
        // Find project root
        let project_root = Self::find_project_root(&current_path);
        
        // Extract mod statements from CST
        let mut module_names = Vec::new();
        for block in &cst.blocks {
            for stmt in &block.node.statements {
                if let CstStatement::Mod { identifier } = &stmt.node {
                    module_names.push(identifier.node.clone());
                }
            }
        }
        
        let mut loaded_file_ids = Vec::new();
        
        // Load each module
        for module_name in module_names {
            // Resolve module path
            let module_path = match Self::resolve_module_path(&module_name, &current_path, project_root.as_deref()) {
                Some(path) => path,
                None => {
                    log::warn!("Module '{}' not found for file {:?}", module_name, current_path);
                    continue;
                }
            };
            
            // Convert to URI
            let module_uri = match Url::from_file_path(&module_path) {
                Ok(uri) => uri,
                Err(_) => {
                    log::warn!("Failed to convert module path to URI: {:?}", module_path);
                    continue;
                }
            };
            
            // Check if already loaded
            let module_file_id = if let Some(existing_id) = source_manager.get_file_id(&module_uri) {
                if loaded_modules.contains(&existing_id) {
                    continue; // Already processed
                }
                existing_id
            } else {
                // Load from disk and register
                let content = std::fs::read_to_string(&module_path)
                    .map_err(|e| format!("Failed to read module file {:?}: {}", module_path, e))?;
                
                // Note: We can't update source_manager here since we only have a read reference
                // This will be handled by the caller after we return the paths
                // For now, we'll return an error if the module isn't already registered
                return Err(format!("Module file {:?} not registered in source manager. This should be handled by caller.", module_path));
            };
            
            loaded_modules.insert(module_file_id);
            loaded_file_ids.push(module_file_id);
            
            log::debug!("Loaded module '{}' from {:?} as file {}", module_name, module_path, module_file_id.0);
            
            // Recursively load modules from this module file
            let module_text = source_manager.get_file_text(module_file_id)
                .ok_or_else(|| format!("Module file {} not found", module_file_id.0))?
                .to_string();
            
            let (module_cst, _) = parse_cst_program(&module_text)
                .map_err(|e| format!("Failed to parse module {:?}: {}", module_path, e))?;
            
            // Recursively discover modules (sync call)
            let nested_modules = self.discover_and_load_modules_sync(module_file_id, &module_cst, loaded_modules, source_manager)?;
            loaded_file_ids.extend(nested_modules);
        }
        
        Ok(loaded_file_ids)
    }

    /// Trigger compilation for changed files.
    ///
    /// **CRITICAL LSP SAFETY RULE**: This function must compile project-wide, not file-by-file.
    /// When ANY file changes, we recompile ALL open files together to maintain consistent state.
    ///
    /// This is called on file open/change to:
    /// 1. Collect ALL open root files (project-wide compilation)
    /// 2. Parse all open files to CST (fail fast on parse errors)
    /// 3. Discover and load modules from mod statements (server-side)
    /// 4. Lower CST to AST for all files
    /// 5. Lower AST to HIR
    /// 6. Collect diagnostics
    /// 7. Build symbol tables and semantic index
    ///
    /// **Returns Ok(()) only if ALL files parsed successfully.**
    /// On parse failure, Err is returned and snapshot is NOT updated (last good snapshot preserved).
    pub async fn compile_changed_files(&self, changed_file_ids: Vec<FileId>) -> Result<(), String> {
        log_to_file(&format!("=== compile_changed_files called with {} changed files ===", changed_file_ids.len()));
        // CRITICAL: Compile ALL open root files together, not just the changed file
        // This prevents mixing CST from file A with HIR from file B
        let root_files = {
            let roots = self.open_roots.read().await;
            if roots.is_empty() {
                // No open roots yet - use changed file
                changed_file_ids
            } else {
                roots.iter().copied().collect::<Vec<_>>()
            }
        };
        
        log_to_file(&format!("Root files to compile: {}", root_files.len()));

        if root_files.is_empty() {
            return Err("No files to compile".to_string());
        }

        // Use first root file as primary entry point for module discovery
        let root_file_id = root_files[0];
        log_to_file(&format!("Primary root file: {:?}", root_file_id));
        
        // CRITICAL: Parse ALL root files before proceeding
        // If ANY file fails to parse, we abort and keep the last good snapshot
        eprintln!("[LSP] Parsing {} root file(s)...", root_files.len());
        
        let mut root_file_csts = HashMap::new();
        let source_manager = self.source_manager.read().await;
        
        for file_id in &root_files {
            let file_text = source_manager.get_file_text(*file_id)
                .ok_or_else(|| format!("File {} not found", file_id.0))?
                .to_string();
            
            eprintln!("[LSP] Parsing root file {} ({} bytes)...", file_id.0, file_text.len());
            let parse_result = parse_cst_program(&file_text);
            
            match parse_result {
                Ok((cst, docs)) => {
                    eprintln!("[LSP] Root file {} parsed successfully", file_id.0);
                    root_file_csts.insert(*file_id, (cst, docs));
                }
                Err(e) => {
                    let error_msg = format!("Parse error in file {}: {}", file_id.0, e);
                    eprintln!("[LSP] Parse failed: {}", error_msg);
                    log::warn!("Parse failed for file {}: {}", file_id.0, error_msg);
                    drop(source_manager);
                    // Return error - caller will publish diagnostics but NOT update snapshot
                    return Err(error_msg);
                }
            }
        }
        drop(source_manager);
        
        // Get primary root file CST for module discovery (keep reference to avoid move)
        let root_cst_ref = &root_file_csts.get(&root_file_id)
            .ok_or_else(|| format!("Root file {} not in parsed files", root_file_id.0))?
            .0;
        
        // Discover and load modules from mod statements
        // This is the key: we load modules server-side, not waiting for LSP events
        log::debug!("Starting module discovery for file {}", root_file_id.0);
        
        // Discover modules (need source_manager for this)
        let (module_paths, module_names) = {
            let source_manager = self.source_manager.read().await;
            let current_uri = source_manager.get_uri(root_file_id).cloned();
            let current_path = match current_uri {
                Some(uri) => uri.to_file_path().map_err(|_| "URI is not a file path".to_string())?,
                None => return Err("Root file URI not found".to_string()),
            };
            let project_root = Self::find_project_root(&current_path);
            
            // Extract mod statements from primary root file
            let mut mod_names = Vec::new();
            for block in &root_cst_ref.blocks {
                for stmt in &block.node.statements {
                    if let CstStatement::Mod { identifier } = &stmt.node {
                        mod_names.push(identifier.node.clone());
                    }
                }
            }
            
            // Resolve module paths
            let mut paths = Vec::new();
            for mod_name in &mod_names {
                if let Some(path) = Self::resolve_module_path(mod_name, &current_path, project_root.as_deref()) {
                    paths.push((mod_name.clone(), path));
                }
            }
            log::info!("Found {} mod statements: {:?}", mod_names.len(), mod_names);
            eprintln!("[LSP] Found {} mod statements in file: {:?}", mod_names.len(), mod_names);
            (paths, mod_names)
        };
        
        log::info!("Resolved {} module paths", module_paths.len());
        eprintln!("[LSP] Resolved {} module paths", module_paths.len());
        
        // Load discovered modules into source manager
        // Include all root files in compilation
        let mut all_file_ids = root_files.clone();
        for (mod_name, module_path) in module_paths {
            let module_uri = Url::from_file_path(&module_path)
                .map_err(|_| format!("Failed to convert module path to URI: {:?}", module_path))?;
            
            let module_file_id = {
                let source_manager = self.source_manager.read().await;
                if let Some(existing_id) = source_manager.get_file_id(&module_uri) {
                    existing_id
                } else {
                    drop(source_manager);
                    // Load from disk
                    let content = std::fs::read_to_string(&module_path)
                        .map_err(|e| format!("Failed to read module file {:?}: {}", module_path, e))?;
                    let content_len = content.len();
                    
                    let mut source_manager = self.source_manager.write().await;
                    let file_id = source_manager.update_file(&module_uri, content, 0);
                    log::info!("Loaded module '{}' from {:?} as file {} ({} bytes)", mod_name, module_path, file_id.0, content_len);
                    file_id
                }
            };
            
            all_file_ids.push(module_file_id);
        }
        
        log::info!("Discovered {} modules, compiling {} files total", all_file_ids.len() - 1, all_file_ids.len());
        eprintln!("[LSP] Discovered {} modules, compiling {} files total", all_file_ids.len() - 1, all_file_ids.len());
        
        // STEP 1: Parse all files (all root files + discovered modules) to CST
        let mut file_csts = HashMap::new();
        // Add all parsed root files
        for (file_id, (cst, docs)) in root_file_csts {
            file_csts.insert(file_id, (cst, docs));
        }
        
        // Parse all discovered modules
        let mut file_asts = HashMap::new();
        let source_manager = self.source_manager.read().await;
        for module_file_id in &all_file_ids {
            if *module_file_id == root_file_id {
                continue; // Already parsed
            }
            
            let module_text = source_manager.get_file_text(*module_file_id)
                .ok_or_else(|| format!("Module file {} not found in source manager", module_file_id.0))?;
            
            let (module_cst, module_cst_docs) = parse_cst_program(module_text)
                .map_err(|e| format!("Failed to parse module file {}: {}", module_file_id.0, e))?;
            
            file_csts.insert(*module_file_id, (module_cst.clone(), module_cst_docs.clone()));
            
            // Lower module CST to AST
            let (module_ast, _module_ast_docs) = lower_cst_to_ast(module_cst, module_cst_docs)
                .map_err(|e| format!("Failed to lower module file {} to AST: {}", module_file_id.0, e))?;
            
            file_asts.insert(*module_file_id, module_ast);
            log::debug!("Parsed and lowered module file {}", module_file_id.0);
        }
        drop(source_manager);
        
        // Lower all root CSTs to AST
        for root_file_id in &root_files {
            let (root_cst, root_cst_docs) = file_csts.get(root_file_id)
                .ok_or_else(|| format!("Root file {} not in CST map", root_file_id.0))?
                .clone();
            
            let (root_ast, _ast_docs) = lower_cst_to_ast(root_cst.clone(), root_cst_docs)
                .map_err(|e| format!("Lowering error for file {}: {}", root_file_id.0, e))?;
            file_asts.insert(*root_file_id, root_ast);
        }
        
        // Use first root AST as primary for HIR building
        let root_ast = file_asts.get(&root_file_id)
            .ok_or_else(|| format!("Root file {} not in AST map", root_file_id.0))?
            .clone();
        
        log::info!("Parsed {} files, lowered {} files to AST", file_csts.len(), file_asts.len());
        eprintln!("[LSP] Parsed {} files, lowered {} files to AST", file_csts.len(), file_asts.len());

        // Perform all compilation work in a block to ensure context is dropped before await
        eprintln!("[LSP] Entering HIR/symbols building block...");
        let (hir, all_diagnostics, symbols, cst_id_to_symbol_id, cst_id_to_span) = {
            eprintln!("[LSP] Inside HIR building block, creating context...");
            // Lower AST to HIR using CompileSession pattern
            // Note: CompileContext requires a 'static function pointer, so we need to
            // use a static check. For LSP, we'll create a simple context.
            let native_descriptors = self.engine.native_function_descriptors();
            
            // Create a static function that checks if an ID is native
            // Native function IDs start at 10000
            fn is_native_function_static(id: u32) -> bool {
                id >= 10000
            }

            let context = CompileContext {
                is_native_function: &is_native_function_static,
                native_functions: native_descriptors,
            };

            let mut hir_builder = HirBuilder::new();
            
            // Phase 3: Set up CST identity tracking
            use crate::core::cst::{CstId, Span as CstSpan};
            use crate::core::hir_lowering::SymbolId;
            let mut cst_id_to_symbol_id_map: std::collections::HashMap<CstId, SymbolId> = std::collections::HashMap::new();
            let mut cst_id_to_span_map: std::collections::HashMap<CstId, CstSpan> = std::collections::HashMap::new();
            
            // Set callback to collect CST identity mappings
            unsafe {
                hir_builder.set_bind_target(&mut cst_id_to_symbol_id_map);
            }
            
            // Also collect spans from CST nodes during lowering
            // We'll populate cst_id_to_span_map by walking the CST after lowering
            
            // Register native functions as builtin functions
            // CRITICAL: Each native function is registered twice - once with qualified name (e.g., "std.print")
            // and once with unqualified name (e.g., "print") for backward compatibility.
            // We need to register BOTH so that code can call functions either way.
            // Since register_builtin_function uses function ID as key, the second registration will overwrite
            // the first, so we'll end up with unqualified names in hir.functions.
            // But both names should be in the symbol table for matching.
            for native in native_descriptors {
                hir_builder.register_builtin_function(
                    &native.name,
                    native.signature.clone(),
                    native.id,
                );
                
                // CRITICAL: If this is a qualified name (e.g., "std.print"), also register with unqualified name
                // This ensures code can call built-in functions directly without importing
                if let Some(dot_pos) = native.name.find('.') {
                    let unqualified_name = &native.name[dot_pos + 1..];
                    // Register again with unqualified name - this overwrites the qualified registration
                    // but that's OK since both refer to the same function ID
                    hir_builder.register_builtin_function(
                        unqualified_name,
                        native.signature.clone(),
                        native.id,
                    );
                }
            }

            // Build stdlib module maps from native_descriptors
            // Functions are registered with qualified names like "std.print", "math.round", etc.
            let mut stdlib_modules: HashMap<String, HashMap<String, u32>> = HashMap::new();
            for native in native_descriptors {
                // Look for qualified names (e.g., "std.print", "math.round")
                if let Some(dot_pos) = native.name.find('.') {
                    let module_name = &native.name[..dot_pos];
                    let function_name = &native.name[dot_pos + 1..];
                    stdlib_modules
                        .entry(module_name.to_string())
                        .or_insert_with(HashMap::new)
                        .insert(function_name.to_string(), native.id);
                }
            }
            
            // Register stdlib modules with their functions
            for (module_name, functions) in stdlib_modules {
                hir_builder.register_module(&module_name, functions, HashMap::new(), HashMap::new());
            }

            // STEP 2: Build HIR from all ASTs (root + modules)
            // Register user-defined modules first
            for (module_file_id, module_ast) in &file_asts {
                if *module_file_id == root_file_id {
                    continue; // Root will be built as __main__
                }
                
                // Extract module name from file path or use file_id as fallback
                // For now, modules are registered without explicit names (will use path later)
                // Just register the functions/types from this AST
                log::debug!("Registering module from file {}", module_file_id.0);
            }
            
            // Build HIR from root AST (main module)
            hir_builder.set_current_module(Some("__main__".to_string()));
            hir_builder.register_module("__main__", HashMap::new(), HashMap::new(), HashMap::new());

            let mut all_diagnostics = Vec::new();
            
            // Build root AST
            eprintln!("[LSP] Building HIR from root AST...");
            eprintln!("[LSP] About to call build_append, root_ast has {} blocks", root_ast.blocks.len());
            eprintln!("[LSP] Calling build_append NOW...");
            let build_start = std::time::Instant::now();
            
            let build_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                eprintln!("[LSP] Inside build_append closure, calling hir_builder.build_append...");
                let result = hir_builder.build_append(&context, root_ast.clone());
                eprintln!("[LSP] build_append returned from hir_builder");
                result
            }));
            
            let build_duration = build_start.elapsed();
            eprintln!("[LSP] build_append call completed in {:?}", build_duration);
            
            match build_result {
                Ok(Ok(())) => {
                    eprintln!("[LSP] HIR build_append succeeded");
                },
                Ok(Err(err)) => {
                    eprintln!("[LSP] HIR build_append failed: {}", err);
                    log::warn!("HIR build failed for root: {}", err);
                    all_diagnostics.push(err);
                },
                Err(_) => {
                    eprintln!("[LSP] CRITICAL: build_append panicked!");
                    log::error!("HIR build_append panicked");
                    // Continue with empty diagnostics - we'll still try to create a snapshot
                }
            }
            eprintln!("[LSP] HIR build_append completed, diagnostics count: {}", all_diagnostics.len());

            // Get HIR even if there are diagnostics (partial compilation)
            // This allows semantic tokens and symbol queries to work with partial data
            eprintln!("[LSP] Building HIR...");
            log_to_file("Building HIR...");
            let hir = if all_diagnostics.is_empty() {
                eprintln!("[LSP] No diagnostics, taking HIR");
                log_to_file("No diagnostics, taking HIR");
                Some(hir_builder.take_ast())
            } else {
                // Try to get partial HIR anyway - may still have useful data
                // Note: take_ast might be incomplete, but better than nothing
                log::debug!("Using partial HIR despite {} diagnostics", all_diagnostics.len());
                eprintln!("[LSP] Using partial HIR despite {} diagnostics", all_diagnostics.len());
                log_to_file(&format!("Using partial HIR despite {} diagnostics", all_diagnostics.len()));
                Some(hir_builder.take_ast())
            };
            eprintln!("[LSP] HIR built: {}", if hir.is_some() { "Some" } else { "None" });
            log_to_file(&format!("HIR built: {}", if hir.is_some() { "Some" } else { "None" }));

            // STEP 3: Build symbol table and semantic index from ALL files
            // Combine CSTs from all files for complete semantic view
            // Note: For now, HIR only contains root file, but we can extract identifiers from all CSTs
            eprintln!("[LSP] Building semantic index...");
            log_to_file("Building semantic index...");
            let symbols = if let Some(ref hir_ast) = hir {
                // Build semantic index using root HIR/AST, but include all CSTs for identifier extraction
                // This gives us identifier spans from all files, even if symbols are only from root
                eprintln!("[LSP] Calling build_semantic_index_multi...");
                log_to_file("Calling build_semantic_index_multi...");
                log_to_file(&format!("file_csts has {} files", file_csts.len()));
                let sym = CompilerState::build_semantic_index_multi(hir_ast, &root_ast, &file_csts);
                eprintln!("[LSP] Semantic index built: {} symbols", sym.symbol_info.len());
                log_to_file(&format!("Semantic index built: {} symbols", sym.symbol_info.len()));
                Some(sym)
            } else {
                eprintln!("[LSP] No HIR, skipping semantic index");
                log_to_file("No HIR, skipping semantic index");
                None
            };
            
            // Phase 3: Collect CST spans by walking all CSTs
            // This populates cst_id_to_span_map for all CST nodes
            eprintln!("[LSP] Collecting CST spans...");
            for (_, (cst, _)) in &file_csts {
                Self::collect_cst_spans(cst, &mut cst_id_to_span_map);
            }
            eprintln!("[LSP] CST spans collected: {} mappings", cst_id_to_span_map.len());
            
            eprintln!("[LSP] Exiting HIR building block, returning results...");
            // Context is dropped here when block ends
            (hir, all_diagnostics, symbols, cst_id_to_symbol_id_map, cst_id_to_span_map)
        };
        eprintln!("[LSP] HIR building block completed, hir={}, symbols={}", 
            if hir.is_some() { "Some" } else { "None" },
            if symbols.is_some() { "Some" } else { "None" });

        // STEP 4: Build CST map for all compiled files (all root files + modules)
        eprintln!("[LSP] Building CST map...");
        let mut csts_map = HashMap::new();
        // Add all CSTs (root files + modules) - already in file_csts
        for (file_id, (cst, _)) in &file_csts {
            csts_map.insert(*file_id, cst.clone());
        }
        eprintln!("[LSP] CST map built: {} files", csts_map.len());
        
        // Build diagnostics map (for now, all diagnostics go to root_file_id)
        // TODO: In the future, track diagnostics per-file
        let mut diagnostics_map = HashMap::new();
        diagnostics_map.insert(root_file_id, all_diagnostics.clone());
        eprintln!("[LSP] Diagnostics map built: {} diagnostics", all_diagnostics.len());

        // Phase 3: Use collected CST identity mappings
        eprintln!("[LSP] Creating CompilerSnapshot...");
        let snapshot = CompilerSnapshot::new(
            csts_map.clone(),
            hir.clone(),
            diagnostics_map.clone(),
            symbols.clone(),
            cst_id_to_symbol_id,
            cst_id_to_span,
        );
        eprintln!("[LSP] CompilerSnapshot created");
        
        // STEP 5: Log snapshot integrity metrics
        let file_count = csts_map.len();
        let symbol_count = symbols.as_ref().map(|s| s.symbol_info.len()).unwrap_or(0);
        let span_to_symbol_count = symbols.as_ref().map(|s| s.span_to_symbol.len()).unwrap_or(0);
        
        // SAFETY CHECK 4: Snapshot must have files and symbols
        if file_count == 0 {
            let msg = "CRITICAL: Snapshot has no files!";
            eprintln!("[LSP] ERROR: {}", msg);
            log::error!("{}", msg);
        }
        if symbol_count == 0 && file_count > 0 {
            let msg = format!("WARNING: Snapshot has {} files but 0 symbols. This may indicate compilation failure.", file_count);
            eprintln!("[LSP] {}", msg);
            log::warn!("{}", msg);
        }
        
        log::info!(
            "snapshot: files={}, symbols={}, span->symbol={}, diagnostics={}",
            file_count,
            symbol_count,
            span_to_symbol_count,
            all_diagnostics.len()
        );
        eprintln!(
            "[LSP] snapshot: files={}, symbols={}, span->symbol={}, diagnostics={}",
            file_count,
            symbol_count,
            span_to_symbol_count,
            all_diagnostics.len()
        );
        
        // CRITICAL: Only update snapshot on successful compilation
        // This ensures we never mix stale CST with new HIR or vice versa
        // The snapshot is atomic: all files, CST, AST, HIR, symbols together
        // CRITICAL: Use lock-free atomic swap - LSP handlers can read without blocking
        // ArcSwap stores Arc<T>, so we wrap the Option in Arc
        
        log::info!("Publishing snapshot: files={}, symbols={}, span->symbol={}", 
            file_count, symbol_count, span_to_symbol_count);
        eprintln!("[LSP] Publishing snapshot: files={}, symbols={}, span->symbol={}", 
            file_count, symbol_count, span_to_symbol_count);
        
        eprintln!("[LSP] Creating Arc<Option<Arc<CompilerSnapshot>>> wrapper...");
        let snapshot_arc = Arc::new(Some(Arc::new(snapshot.clone())));
        eprintln!("[LSP] Snapshot Arc created, pointer: {:p}", snapshot_arc);
        
        eprintln!("[LSP] Storing snapshot to ArcSwap...");
        self.snapshot.store(snapshot_arc.clone());
        eprintln!("[LSP] Snapshot store() call completed");
        
        // Immediately verify the snapshot was stored
        eprintln!("[LSP] Verifying snapshot was stored...");
        let loaded = self.snapshot.load();
        eprintln!("[LSP] Loaded snapshot from ArcSwap");
        if let Some(ref loaded_snapshot) = **loaded {
            let file_count = loaded_snapshot.file_ids().count();
            let symbol_count = loaded_snapshot.symbol_table().map(|s| s.symbol_info.len()).unwrap_or(0);
            eprintln!("[LSP] Snapshot verification: SUCCESS - snapshot contains {} files, {} symbols", 
                file_count, symbol_count);
            log::info!("Snapshot stored successfully and verified: {} files, {} symbols", file_count, symbol_count);
        } else {
            let msg = "CRITICAL: Snapshot store failed - snapshot is None after store!";
            eprintln!("[LSP] ERROR: {}", msg);
            log::error!("{}", msg);
            panic!("{}", msg);
        }
        eprintln!("[LSP] Snapshot stored successfully and verified!");
        
        // Also store last good snapshot per file for fallback
        // This allows us to provide semantic tokens even if other files are broken
        for file_id in &all_file_ids {
            let mut last_good = self.last_good_snapshots.write().await;
            last_good.insert(*file_id, snapshot.clone());
        }

        Ok(())
    }
    
    /// Get snapshot for semantic queries (tokens, hover, goto).
    /// Returns last good snapshot if current compilation failed for this file.
    /// 
    /// CRITICAL: This uses lock-free reads via ArcSwap - never blocks on compilation.
    pub async fn get_snapshot_for_file(&self, file_id: FileId) -> Option<CompilerSnapshot> {
        // Lock-free read - never blocks even if compilation is writing
        // ArcSwap.load() returns Arc<Option<Arc<CompilerSnapshot>>>
        let snapshot_arc = self.snapshot.load();
        
        // Dereference to get Option<Arc<CompilerSnapshot>>
        // Check if current snapshot is valid for this file
        if let Some(ref current) = **snapshot_arc {
            // current is &Arc<CompilerSnapshot>, so we need to dereference and clone
            // Current snapshot is valid if it has CST for this file and file parses successfully
            if current.cst(file_id).is_some() {
                return Some((**current).clone());
            }
        }
        
        // Fall back to last good snapshot for this file
        // Note: This still uses RwLock, but it's a separate lock so won't block compilation
        // CRITICAL: This is the only remaining lock in the read path - but it's separate from compilation
        let last_good = self.last_good_snapshots.read().await;
        last_good.get(&file_id).cloned()
    }
}

/// Extract all identifier spans from CST.
/// Helper function to build identifier -> spans map for semantic indexing.
fn extract_cst_identifier_spans(cst: &CstProgram) -> HashMap<String, Vec<HirSpan>> {
    let mut span_map: HashMap<String, Vec<HirSpan>> = HashMap::new();
    
    fn walk_expr(expr: &Spanned<CstExpr>, span_map: &mut HashMap<String, Vec<HirSpan>>) {
        match &expr.node {
            CstExpr::Identifier(name) => {
                let hir_span = HirSpan::new(name.span.start as usize, name.span.end as usize);
                span_map.entry(name.node.clone())
                    .or_insert_with(Vec::new)
                    .push(hir_span);
            }
            CstExpr::FunctionCall { callee, arguments, .. } => {
                walk_expr(callee, span_map);
                for arg in arguments {
                    match &arg.node {
                        crate::core::cst::CstCallArgument::Expr(expr) => walk_expr(expr, span_map),
                        _ => {}
                    }
                }
            }
            CstExpr::Infix { lhs, rhs, .. } => {
                walk_expr(lhs, span_map);
                walk_expr(rhs, span_map);
            }
            CstExpr::Prefix { rhs, .. } => {
                walk_expr(rhs, span_map);
            }
            CstExpr::Postfix { lhs, .. } => {
                walk_expr(lhs, span_map);
            }
            CstExpr::MemberAccess { object, .. } => {
                walk_expr(object, span_map);
            }
            CstExpr::FieldAccess { object, .. } => {
                walk_expr(object, span_map);
            }
            CstExpr::ArrayIndex { array, indices, .. } => {
                walk_expr(array, span_map);
                for idx_spec in indices {
                    match &idx_spec.node {
                        crate::core::cst::CstIndexSpec::Single(expr) => walk_expr(expr, span_map),
                        crate::core::cst::CstIndexSpec::Range { start, end, step, .. } => {
                            if let Some(expr) = start {
                                walk_expr(expr, span_map);
                            }
                            if let Some(expr) = end {
                                walk_expr(expr, span_map);
                            }
                            if let Some((_, expr)) = step {
                                walk_expr(expr, span_map);
                            }
                        }
                        crate::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                            if let Some(expr) = start {
                                walk_expr(expr, span_map);
                            }
                            if let Some(expr) = end {
                                walk_expr(expr, span_map);
                            }
                        }
                    }
                }
            }
            CstExpr::Compose { lhs, rhs, .. } => {
                walk_expr(lhs, span_map);
                walk_expr(rhs, span_map);
            }
            CstExpr::Group { inner, .. } => {
                walk_expr(inner, span_map);
            }
            CstExpr::Loop { init_vars, body, .. } => {
                for (_, _, expr) in init_vars {
                    walk_expr(expr, span_map);
                }
                walk_block(&body, span_map);
            }
            CstExpr::StructInit { fields, .. } => {
                for field in fields {
                    walk_expr(&field.node.value, span_map);
                }
            }
            CstExpr::Array { elements, .. } => {
                for elem in elements {
                    walk_expr(elem, span_map);
                }
            }
            CstExpr::Closure { arguments: _, body, .. } => {
                // Walk closure body (arguments handled separately)
                match body {
                    crate::core::cst::CstClosureBody::Expression(expr) => walk_expr(expr, span_map),
                    crate::core::cst::CstClosureBody::Block(block) => walk_block(block, span_map),
                }
            }
            _ => {}
        }
    }
    
    fn walk_block(block: &crate::core::cst::Spanned<crate::core::cst::CstBlock>, span_map: &mut HashMap<String, Vec<HirSpan>>) {
        for stmt in &block.node.statements {
            match &stmt.node {
                CstStatement::Expression(expr) => walk_expr(expr, span_map),
                CstStatement::Let { expression, .. } => walk_expr(expression, span_map),
                CstStatement::Const { expression, .. } => walk_expr(expression, span_map),
                CstStatement::Assign { expression, .. } => walk_expr(expression, span_map),
                CstStatement::AssignIncrement { expression, .. } => walk_expr(expression, span_map),
                CstStatement::AssignDecrement { expression, .. } => walk_expr(expression, span_map),
                _ => {}
            }
        }
    }
    
    for block in &cst.blocks {
        for stmt in &block.node.statements {
            use crate::core::cst::CstStatement;
            match &stmt.node {
                CstStatement::Let { identifier, expression, .. } => {
                    let hir_span = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                    span_map.entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(hir_span);
                    walk_expr(expression, &mut span_map);
                }
                CstStatement::Assign { identifier, expression, .. } => {
                    let hir_span = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                    span_map.entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(hir_span);
                    walk_expr(expression, &mut span_map);
                }
                CstStatement::FunctionDeclaration { identifier, body, .. } => {
                    let hir_span = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                    span_map.entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(hir_span);
                    // Body is a block, walk its statements
                    walk_block(body, &mut span_map);
                }
                CstStatement::If { arms, else_block, .. } => {
                    for (condition, block) in arms {
                        walk_expr(condition, &mut span_map);
                        walk_block(block, &mut span_map);
                    }
                    if let Some(block) = else_block {
                        walk_block(block, &mut span_map);
                    }
                }
                CstStatement::Match { expression, cases, .. } => {
                    walk_expr(expression, &mut span_map);
                    for (pattern, block) in cases {
                        if let Some(expr) = pattern {
                            walk_expr(expr, &mut span_map);
                        }
                        walk_block(block, &mut span_map);
                    }
                }
                CstStatement::Return { expression, .. } => {
                    walk_expr(expression, &mut span_map);
                }
                CstStatement::Break { expression, .. } => {
                    if let Some(expr) = expression {
                        walk_expr(expr, &mut span_map);
                    }
                }
                CstStatement::Loop { init_vars, body, .. } => {
                    for (_, _, expr) in init_vars {
                        walk_expr(expr, &mut span_map);
                    }
                    walk_block(body, &mut span_map);
                }
                CstStatement::While { condition, body, .. } => {
                    walk_expr(condition, &mut span_map);
                    walk_block(body, &mut span_map);
                }
                CstStatement::For { start, end, body, .. } => {
                    walk_expr(start, &mut span_map);
                    walk_expr(end, &mut span_map);
                    walk_block(body, &mut span_map);
                }
                CstStatement::Expression(expr) => {
                    walk_expr(expr, &mut span_map);
                }
                _ => {}
            }
        }
    }
    
    span_map
}

impl CompilerState {
    /// Build semantic index from symbol table and CST.
    /// 
    /// Creates mappings:
    /// - span_to_symbol: Maps each span to its symbol
    /// - symbol_to_definition: Maps symbol to its definition span
    /// - symbol_to_references: Maps symbol to all spans (definitions + references)
    /// - symbol_info: Maps symbol to its information (name, kind, type)
    fn build_semantic_index_multi(
        hir: &HirAst,
        ast: &Program,
        file_csts: &HashMap<FileId, (CstProgram, std::collections::HashMap<crate::core::cst::Span, crate::core::cst::DocBlock>)>,
    ) -> SymbolTable {
        eprintln!("[LSP] [DEBUG] build_semantic_index_multi: file_csts has {} files", file_csts.len());
        log_to_file(&format!("build_semantic_index_multi: file_csts has {} files", file_csts.len()));

        // For now, use the root CST (first file) for symbol building
        // TODO: Support multi-file symbol resolution
        if let Some((root_cst, _)) = file_csts.values().next() {
            eprintln!("[LSP] [DEBUG] Found root CST, calling build_semantic_index...");
            log_to_file("Found root CST, calling build_semantic_index...");
            let result = Self::build_semantic_index(hir, ast, root_cst);
            log_to_file(&format!("build_semantic_index returned {} symbols", result.symbol_info.len()));
            result
        } else {
            eprintln!("[LSP] [DEBUG] NO CSTs found! Returning empty symbol table");
            log_to_file("NO CSTs found! Returning empty symbol table");
            // Fallback: build empty table if no CSTs
            use crate::core::lsp_api::SymbolTable;
            SymbolTable {
                span_to_symbol: HashMap::new(),
                symbol_to_definition: HashMap::new(),
                symbol_to_references: HashMap::new(),
                symbol_info: HashMap::new(),
            }
        }
    }

    /// Phase 3: Collect all CST node spans into a map.
    /// Walks the CST tree and records CstId → Span mappings.
    fn collect_cst_spans(
        cst: &CstProgram,
        span_map: &mut std::collections::HashMap<crate::core::cst::CstId, crate::core::cst::Span>,
    ) {
        use crate::core::cst::{CstExpr, CstStatement};
        
        fn walk_expr(expr: &crate::core::cst::Spanned<CstExpr>, span_map: &mut std::collections::HashMap<crate::core::cst::CstId, crate::core::cst::Span>) {
            // Record this expression's span
            span_map.insert(expr.id, expr.span);
            
            // Recurse into children
            match &expr.node {
                CstExpr::Identifier(_) | CstExpr::Literal(_) => {
                    // Leaf nodes - already recorded
                }
                CstExpr::Infix { lhs, rhs, .. } => {
                    walk_expr(lhs, span_map);
                    walk_expr(rhs, span_map);
                }
                CstExpr::Prefix { rhs, .. } => {
                    walk_expr(rhs, span_map);
                }
                CstExpr::Postfix { lhs, .. } => {
                    walk_expr(lhs, span_map);
                }
                CstExpr::FunctionCall { callee, arguments, .. } => {
                    walk_expr(callee, span_map);
                    for arg in arguments {
                        match &arg.node {
                            crate::core::cst::CstCallArgument::Expr(expr) => {
                                walk_expr(expr, span_map);
                            }
                            crate::core::cst::CstCallArgument::Hole(_) => {}
                        }
                    }
                }
                CstExpr::PartialCall { func, args, .. } => {
                    walk_expr(func, span_map);
                    for arg in args {
                        match &arg.node {
                            crate::core::cst::CstCallArgument::Expr(expr) => {
                                walk_expr(expr, span_map);
                            }
                            crate::core::cst::CstCallArgument::Hole(_) => {}
                        }
                    }
                }
                CstExpr::MemberAccess { object, .. } => {
                    walk_expr(object, span_map);
                }
                CstExpr::Compose { lhs, rhs, .. } => {
                    walk_expr(lhs, span_map);
                    walk_expr(rhs, span_map);
                }
                CstExpr::FieldAccess { object, .. } => {
                    walk_expr(object, span_map);
                }
                CstExpr::Array { elements, .. } => {
                    for elem in elements {
                        walk_expr(elem, span_map);
                    }
                }
                CstExpr::ArrayIndex { array, indices, .. } => {
                    walk_expr(array, span_map);
                    for idx in indices {
                        match &idx.node {
                            crate::core::cst::CstIndexSpec::Single(expr) => {
                                walk_expr(expr, span_map);
                            }
                            crate::core::cst::CstIndexSpec::Range { start, end, .. } => {
                                if let Some(start) = start {
                                    walk_expr(start, span_map);
                                }
                                if let Some(end) = end {
                                    walk_expr(end, span_map);
                                }
                            }
                            crate::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                                if let Some(start) = start {
                                    walk_expr(start, span_map);
                                }
                                if let Some(end) = end {
                                    walk_expr(end, span_map);
                                }
                            }
                        }
                    }
                }
                CstExpr::Group { inner, .. } => {
                    walk_expr(inner, span_map);
                }
                CstExpr::Closure { arguments, body, .. } => {
                    for arg in arguments {
                        span_map.insert(arg.id, arg.span);
                        if !arg.node.is_placeholder {
                            span_map.insert(arg.node.identifier.id, arg.node.identifier.span);
                        }
                    }
                    match body {
                        crate::core::cst::CstClosureBody::Expression(expr) => {
                            walk_expr(expr, span_map);
                        }
                        crate::core::cst::CstClosureBody::Block(block) => {
                            walk_block(block, span_map);
                        }
                    }
                }
                _ => {
                    // Other expression types
                }
            }
        }
        
        fn walk_block(block: &crate::core::cst::CstBlock, span_map: &mut std::collections::HashMap<crate::core::cst::CstId, crate::core::cst::Span>) {
            for stmt in &block.statements {
                span_map.insert(stmt.id, stmt.span);
                walk_stmt(&stmt.node, span_map);
            }
        }
        
        fn walk_stmt(stmt: &CstStatement, span_map: &mut std::collections::HashMap<crate::core::cst::CstId, crate::core::cst::Span>) {
            match stmt {
                CstStatement::Let { identifier, expression, .. } => {
                    span_map.insert(identifier.id, identifier.span);
                    walk_expr(expression, span_map);
                }
                CstStatement::Const { identifier, expression, .. } => {
                    span_map.insert(identifier.id, identifier.span);
                    walk_expr(expression, span_map);
                }
                CstStatement::FunctionDeclaration { identifier, arguments, body, .. } => {
                    span_map.insert(identifier.id, identifier.span);
                    for arg in arguments {
                        span_map.insert(arg.id, arg.span);
                        span_map.insert(arg.node.identifier.id, arg.node.identifier.span);
                    }
                    walk_block(body, span_map);
                }
                CstStatement::Expression(expr) => {
                    walk_expr(expr, span_map);
                }
                _ => {
                    // Other statement types
                }
            }
        }
        
        // Walk all blocks
        for block in &cst.blocks {
            span_map.insert(block.id, block.span);
            walk_block(&block.node, span_map);
        }
    }

    fn build_semantic_index(hir: &HirAst, ast: &Program, cst: &CstProgram) -> SymbolTable {
        use crate::core::lsp_api::SymbolInfo;
        use crate::core::hir_lowering::SymbolId;

        eprintln!("[LSP] [DEBUG] build_semantic_index called with CST having {} blocks", cst.blocks.len());
        log_to_file(&format!("build_semantic_index called with CST having {} blocks", cst.blocks.len()));

        // Build the symbol table (has defined_at spans)
        eprintln!("[LSP] [DEBUG] About to call build_symbol_table...");
        log_to_file("About to call build_symbol_table...");
        let symbol_table = build_symbol_table(hir, ast, cst);
        eprintln!("[LSP] [DEBUG] build_symbol_table returned {} symbols", symbol_table.symbols.len());
        log_to_file(&format!("build_symbol_table returned {} symbols", symbol_table.symbols.len()));
        
        // Extract all identifier spans from CST by walking it
        // Use the comprehensive extraction function from hir_lowering
        use crate::core::hir_lowering::symbols::extract_identifier_spans_from_cst;
        let span_map_raw = extract_identifier_spans_from_cst(cst);
        // Convert from HashMap<String, Vec<Span>> to HashMap<String, Vec<HirSpan>>
        let span_map: HashMap<String, Vec<HirSpan>> = span_map_raw.into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|s| HirSpan::new(s.start, s.end)).collect()))
            .collect();
        
        // Debug: Count identifier spans
        let total_identifier_spans: usize = span_map.values().map(|v| v.len()).sum();
        let unique_identifiers = span_map.len();
        
        let mut span_to_symbol = HashMap::new();
        let mut symbol_to_definition = HashMap::new();
        let mut symbol_to_references: HashMap<SymbolId, Vec<HirSpan>> = HashMap::new();
        let mut symbol_info: HashMap<SymbolId, SymbolInfo> = HashMap::new();
        
        // Build a name-to-symbol mapping for fast lookup
        let mut name_to_symbol: HashMap<String, Vec<&_>> = HashMap::new();
        for symbol in &symbol_table.symbols {
            name_to_symbol.entry(symbol.name.clone())
                .or_insert_with(Vec::new)
                .push(symbol);
        }
        
        // Process each symbol - add definitions first
        for symbol in &symbol_table.symbols {
            // Determine symbol stability
            use crate::core::lsp_api::SymbolStability;
            let stability = if symbol.defined_at.is_some() {
                SymbolStability::UserDefined
            } else {
                SymbolStability::Unstable
            };
            
            // Store symbol info
            symbol_info.insert(symbol.id, SymbolInfo {
                name: symbol.name.clone(),
                kind: symbol.kind.clone(),
                ty: symbol.ty.clone(),
                stability,
            });
            
            // Add definition span
            if let Some(def_span) = symbol.defined_at {
                symbol_to_definition.insert(symbol.id, def_span);
                span_to_symbol.insert(def_span, symbol.id);
                
                // Initialize references list with definition
                symbol_to_references.entry(symbol.id)
                    .or_insert_with(Vec::new)
                    .push(def_span);
            }
        }
        
        // STEP 3: Map ALL identifier spans from CST to symbols (references)
        // This is critical - we must map every identifier span, not just definitions
        let mut unmapped_names: Vec<String> = Vec::new();
        for (identifier_name, spans) in &span_map {
            // Find all symbols with this name (exact match first)
            let mut matching_symbols: Vec<&_> = if let Some(symbols_with_name) = name_to_symbol.get(identifier_name) {
                symbols_with_name.iter().collect()
            } else {
                Vec::new()
            };
            
            // CRITICAL: Also match built-in functions by unqualified name
            // Built-in functions are registered with qualified names (e.g., "std.print")
            // but code calls them with unqualified names (e.g., "print")
            // So we need to check if the identifier matches any symbol's unqualified name
            if matching_symbols.is_empty() {
                // Try to match by unqualified name (for built-in functions)
                for (symbol_name, symbols) in &name_to_symbol {
                    // Check if symbol name is qualified (e.g., "std.print") and matches identifier
                    if let Some(dot_pos) = symbol_name.find('.') {
                        let unqualified_name = &symbol_name[dot_pos + 1..];
                        if unqualified_name == identifier_name {
                            matching_symbols.extend(symbols.iter());
                            break; // Take the first match
                        }
                    }
                }
            }
            
            if !matching_symbols.is_empty() {
                // CRITICAL: Pick the best symbol for this identifier (only once per identifier, not per span)
                // Prefer symbols with definitions over built-in symbols without definitions
                let best_symbol = matching_symbols.iter()
                    .find(|s| s.defined_at.is_some()) // Prefer symbols with definition spans
                    .or_else(|| matching_symbols.first()); // Otherwise pick the first (likely a built-in)
                
                if let Some(symbol) = best_symbol {
                    // Match ALL spans for this identifier to the same symbol
                    // This ensures both declarations and usages are mapped
                    for span in spans {
                        // Don't add if already mapped (might have been mapped as a definition earlier)
                        if !span_to_symbol.contains_key(span) {
                            span_to_symbol.insert(*span, symbol.id);
                            symbol_to_references.entry(symbol.id)
                                .or_insert_with(Vec::new)
                                .push(*span);
                        } else {
                            // Already mapped - ensure it's in references
                            symbol_to_references.entry(symbol.id)
                                .or_insert_with(Vec::new)
                                .push(*span);
                        }
                    }
                }
            } else {
                // No symbol found for this identifier name
                if !unmapped_names.contains(identifier_name) {
                    unmapped_names.push(identifier_name.clone());
                }
            }
        }
        
        // Diagnostic: Show which identifiers don't have symbols
        if !unmapped_names.is_empty() {
            let sample = &unmapped_names[..unmapped_names.len().min(10)];
            eprintln!("[LSP] Identifiers with no matching symbols (sample of {}): {:?}", unmapped_names.len(), sample);
            
            // Also show which symbols we DO have (for comparison)
            let symbol_names: Vec<String> = name_to_symbol.keys().cloned().collect();
            eprintln!("[LSP] Available symbol names (sample of {}): {:?}", symbol_names.len(), &symbol_names[..symbol_names.len().min(20)]);
        }
        
        // Debug: Show which identifiers were matched
        let matched_identifier_names: std::collections::HashSet<String> = span_map.iter()
            .filter(|(_name, spans)| {
                spans.iter().any(|span| span_to_symbol.contains_key(span))
            })
            .map(|(name, _)| name.clone())
            .collect();
        
        if !matched_identifier_names.is_empty() {
            let matched_sample: Vec<String> = matched_identifier_names.iter().cloned().take(15).collect();
            eprintln!("[LSP] ✓ Matched identifiers (sample of {}): {:?}", matched_identifier_names.len(), matched_sample);
        }
        
        // STEP 4: Report semantic index statistics and enforce safety checks
        let symbol_count = symbol_table.symbols.len();
        let span_to_symbol_count = span_to_symbol.len();
        
        // SAFETY CHECK 1: CST must have identifiers
        if total_identifier_spans == 0 {
            let msg = format!("CRITICAL: No identifier spans extracted from CST! File may be empty or parser failed.");
            eprintln!("[LSP] ERROR: {}", msg);
            log::error!("{}", msg);
        }
        
        // SAFETY CHECK 2: Mapping coverage must be reasonable (80% threshold)
        let coverage_ratio = if total_identifier_spans > 0 {
            span_to_symbol_count as f64 / total_identifier_spans as f64
        } else {
            0.0
        };
        let coverage_pct = coverage_ratio * 100.0;
        
        // Log to both stderr (for debugging) and make it visible
        eprintln!(
            "[LSP] semantic index: cst_identifiers={}, symbols={}, span_to_symbol={}, unique_identifiers={}, coverage={:.1}%",
            total_identifier_spans,
            symbol_count,
            span_to_symbol_count,
            unique_identifiers,
            coverage_pct,
        );
        log::info!(
            "semantic index: cst_identifiers={}, symbols={}, span_to_symbol={}, unique_identifiers={}, coverage={:.1}%",
            total_identifier_spans,
            symbol_count,
            span_to_symbol_count,
            unique_identifiers,
            coverage_pct,
        );
        
        // SAFETY CHECK 3: Log coverage but don't fall back - always use semantic tokens
        // This was useful during debugging, but now we want to see what's unresolved
        if total_identifier_spans > 0 && coverage_ratio < 0.8 {
            let unmapped_count = total_identifier_spans - span_to_symbol_count;
            eprintln!(
                "[LSP] INFO: Semantic index coverage: {:.1}% ({} / {} spans mapped). {} unresolved identifiers.",
                coverage_pct, span_to_symbol_count, total_identifier_spans, unmapped_count
            );
            log::debug!(
                "Semantic index coverage: {:.1}% ({} / {} spans mapped)",
                coverage_pct, span_to_symbol_count, total_identifier_spans
            );
        } else if total_identifier_spans > 0 {
            eprintln!(
                "[LSP] ✓ Semantic index coverage: {:.1}% ({} / {} spans mapped)",
                coverage_pct, span_to_symbol_count, total_identifier_spans
            );
        }
        
        // STEP 5: Rule 3 - Every identifier must have a semantic fate
        // Collect all unique spans from span_map and verify each is mapped
        let mut all_cst_spans = HashSet::new();
        for spans in span_map.values() {
            for span in spans {
                all_cst_spans.insert(*span);
            }
        }
        
        let unresolved_count = all_cst_spans.len() - span_to_symbol_count;
        
        // Log detailed diagnostics for unresolved spans
        if unresolved_count > 0 {
            eprintln!(
                "[LSP] WARNING: {} unresolved identifier spans (out of {} total spans, {:.1}% coverage)",
                unresolved_count, total_identifier_spans, coverage_pct
            );
            
            // Sample first 20 unresolved spans for debugging
            let mut sample_count = 0;
            for span in &all_cst_spans {
                if !span_to_symbol.contains_key(span) {
                    sample_count += 1;
                    if sample_count <= 20 {
                        // Try to find which identifier this span corresponds to
                        for (name, spans) in &span_map {
                            if spans.contains(span) {
                                log::warn!("Unresolved identifier span: {:?} (name: '{}')", span, name);
                                break;
                            }
                        }
                    }
                }
            }
            if unresolved_count > 20 {
                log::warn!("... and {} more unresolved spans (total: {})", unresolved_count - 20, unresolved_count);
            }
        }
        
        // CRITICAL: Log the mapping coverage
        let coverage_pct = if total_identifier_spans > 0 {
            (span_to_symbol_count as f64 / total_identifier_spans as f64) * 100.0
        } else {
            0.0
        };
        eprintln!(
            "[LSP] Mapping coverage: {:.1}% ({} / {} spans mapped)",
            coverage_pct, span_to_symbol_count, total_identifier_spans
        );
        
        SymbolTable {
            span_to_symbol,
            symbol_to_definition,
            symbol_to_references,
            symbol_info,
        }
    }

    /// Get current compiler snapshot.
    /// 
    /// CRITICAL: This uses lock-free reads via ArcSwap - never blocks on compilation.
    pub async fn get_snapshot(&self) -> Option<CompilerSnapshot> {
        // Lock-free read - never blocks even if compilation is writing
        // ArcSwap.load() returns Arc<Option<Arc<CompilerSnapshot>>>
        let snapshot_arc = self.snapshot.load();
        // Dereference to get Option<Arc<CompilerSnapshot>>, then map to clone the inner CompilerSnapshot
        (**snapshot_arc).as_ref().map(|arc| (**arc).clone())
    }

    /// Get source manager reference.
    pub fn source_manager(&self) -> Arc<RwLock<SourceManager>> {
        self.source_manager.clone()
    }
}
