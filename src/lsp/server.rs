use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::core::engine::Engine;
use crate::core::hir_lowering::CompilerState;
use crate::stdlib;

use super::text_utils;
use super::hover;
use super::diagnostics;
use super::semantic_tokens;
use super::completion;

/// Language Server Protocol server for CantaLoop.
/// 
/// Provides IDE features including:
/// - Real-time diagnostics (parse errors, type errors)
/// - Hover information (variable types, function signatures)
/// - Code completion
/// 
/// Uses the compiler's CompilerState as the single source of truth.
/// The LSP never invents language semantics - it only consumes compiler state.
pub struct CantaLoopLSPServer {
    client: Client,
    documents: Arc<tokio::sync::RwLock<HashMap<Url, String>>>,
    compiler_state_cache: Arc<tokio::sync::RwLock<HashMap<Url, CompilerState>>>,
    engine: Arc<Engine>, // Shared engine with built-in functions registered
}

impl CantaLoopLSPServer {
    pub fn new(client: Client) -> Self {
        // Create and initialize engine with standard library
        let mut engine = Engine::new();
        
        // Load all standard library modules
        stdlib::load_all_stdlib(&mut engine);

        Self {
            client,
            documents: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            compiler_state_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            engine: Arc::new(engine),
        }
    }

    fn find_project_root(uri: &Url) -> Option<std::path::PathBuf> {
        // Convert URI to file path
        let file_path = uri.to_file_path().ok()?;
        
        // Walk up the directory tree looking for melon.json
        let mut current = file_path.parent()?;
        loop {
            let melon_json = current.join("melon.json");
            if melon_json.exists() {
                return Some(current.to_path_buf());
            }
            
            if let Some(parent) = current.parent() {
                current = parent;
            } else {
                return None;
            }
        }
    }

    async fn rebuild_compiler_state(&self, uri: &Url, text: &str) {
        // Find project root if this file is part of a project
        let project_root = Self::find_project_root(uri);
        
        // Use the compiler to build state - single source of truth
        match self.engine.compile_for_lsp(text, project_root.as_deref()) {
            Ok(state) => {
                let mut cache = self.compiler_state_cache.write().await;
                cache.insert(uri.clone(), state);
                self.client
                    .log_message(MessageType::INFO, format!("Compiler state built successfully for {}", uri))
                    .await;
            }
            Err(e) => {
                // If compilation fails, remove from cache
                let mut cache = self.compiler_state_cache.write().await;
                cache.remove(uri);
                self.client
                    .log_message(MessageType::WARNING, format!("Compilation failed: {:?}", e))
                    .await;
            }
        }
    }

    async fn update_diagnostics(&self, uri: Url) {
        let documents = self.documents.read().await;
        let text = match documents.get(&uri) {
            Some(text) => text.clone(),
            None => return,
        };
        drop(documents);

        let mut diagnostics_list = Vec::new();

        // Find project root if this file is part of a project
        let project_root = Self::find_project_root(&uri);
        
        // Use compiler state - single source of truth
        match self.engine.compile_for_lsp(&text, project_root.as_deref()) {
            Ok(state) => {
                // Add diagnostics from compiler state
                for error in &state.diagnostics {
                    let error_msg = diagnostics::format_hir_error(error);
                    let (found_line, found_col) = diagnostics::find_error_location(&text, error);
                    
                    // Check if this is a nested invoke pattern error - these should be warnings, not errors
                    let is_nested_invoke_error = error_msg.contains("Confusing nested invoke pattern") ||
                                                 error_msg.contains("nested invoke pattern");
                    
                    let diagnostic = if is_nested_invoke_error {
                        // Convert to warning for nested invoke patterns since code is still runnable
                        Diagnostic {
                            range: text_utils::create_range(found_line, found_col, 1),
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("nested_invoke".to_string())),
                            code_description: None,
                            source: Some("CantaLoop".to_string()),
                            message: error_msg,
                            related_information: None,
                            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                            data: None,
                        }
                    } else {
                        // Regular semantic errors remain as errors
                        diagnostics::create_diagnostic(
                            text_utils::create_range(found_line, found_col, 1),
                            error_msg,
                        )
                    };
                    diagnostics_list.push(diagnostic);
                }
                
                // Check for unused variables using compiler state
                let unused_vars = diagnostics::find_unused_variables(&state);
                for (var_name, line_num, col) in unused_vars {
                    if line_num > 0 || col > 0 { // Only add if we have location info
                        let diagnostic = Diagnostic {
                            range: text_utils::create_range(line_num, col, var_name.len()),
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("unused_variable".to_string())),
                            code_description: None,
                            source: Some("CantaLoop".to_string()),
                            message: format!("Variable '{}' is declared but never used", var_name),
                            related_information: None,
                            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                            data: None,
                        };
                        diagnostics_list.push(diagnostic);
                    }
                }
            }
            Err(e) => {
                // Parse errors
                let (line, col) = match e.location {
                    pest::error::InputLocation::Pos(pos) => text_utils::byte_position_to_line_col(&text, pos),
                    pest::error::InputLocation::Span((start, _end)) => text_utils::byte_position_to_line_col(&text, start),
                };

                // Improve error message for missing type annotations
                let error_msg = format!("{}", e);
                let improved_msg = diagnostics::improve_parse_error_message(&text, line, &error_msg);
                let diagnostic = diagnostics::create_diagnostic(
                    text_utils::create_range(line, col, 1),
                    improved_msg,
                );
                diagnostics_list.push(diagnostic);
            }
        }

        // Check for nested ! invoke patterns (e.g., mul2(add10(i)!)! or mul2!(add10!(i))!)
        let nested_invoke_issues = diagnostics::find_nested_invoke_patterns(&text);
        for (line_num, col, length) in nested_invoke_issues {
            let diagnostic = Diagnostic {
                range: text_utils::create_range(line_num, col, length),
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("nested_invoke".to_string())),
                code_description: None,
                source: Some("CantaLoop".to_string()),
                message: "Nested invoke operator (!) detected. Patterns like `mul2(add10(i)!)!` or `mul2!(add10!(i))!` can be confusing and may create unnecessary intermediate thunks. Consider extracting the inner invocation: `let temp = add10(i)!; mul2(temp)!`".to_string(),
                related_information: None,
                tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                data: None,
            };
            diagnostics_list.push(diagnostic);
        }

        self.client
            .publish_diagnostics(uri, diagnostics_list, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for CantaLoopLSPServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "CantaLoop LSP".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        will_save: None,
                        will_save_wait_until: None,
                        save: None,
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".", "(", "p", "r", "i", "n", "t"].iter().map(|s| s.to_string()).collect()),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: Default::default(),
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::FUNCTION,
                                    SemanticTokenType::VARIABLE,
                                    SemanticTokenType::TYPE,
                                    SemanticTokenType::OPERATOR,
                                    SemanticTokenType::KEYWORD,
                                ],
                                token_modifiers: vec![
                                    SemanticTokenModifier::DECLARATION,
                                    SemanticTokenModifier::READONLY,
                                    SemanticTokenModifier::DEPRECATED, // Reuse deprecated as "thunk" indicator
                                ],
                            },
                            range: Some(true),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "CantaLoop LSP initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text.clone();

        let mut documents = self.documents.write().await;
        documents.insert(uri.clone(), text.clone());
        drop(documents);

        self.rebuild_compiler_state(&uri, &text).await;
        self.update_diagnostics(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let mut documents = self.documents.write().await;
        
        if let Some(text) = params.content_changes.into_iter().next() {
            let text_clone = text.text.clone();
            documents.insert(uri.clone(), text.text);
            drop(documents);
            self.rebuild_compiler_state(&uri, &text_clone).await;
        } else {
            drop(documents);
        }

        self.update_diagnostics(uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let mut documents = self.documents.write().await;
        documents.remove(&uri);
        drop(documents);
        
        let mut cache = self.compiler_state_cache.write().await;
        cache.remove(&uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.client
            .log_message(MessageType::INFO, "Hover method called")
            .await;
        
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let documents = self.documents.read().await;
        let text = match documents.get(&uri) {
            Some(text) => text,
            None => return Ok(None),
        };

        // Simple implementation: find identifier at position
        let identifier_info = match text_utils::extract_identifier_at_position(text, pos.line as usize, pos.character as usize) {
            Some((id, start, end)) => (id, start, end),
            None => return Ok(None),
        };
        let (identifier, start, end) = identifier_info;
        drop(documents);

        // Log for debugging
        self.client
            .log_message(MessageType::INFO, format!("Hover requested for identifier: '{}' at line {}", identifier, pos.line))
            .await;

        // Use compiler state - single source of truth
        let cache = self.compiler_state_cache.read().await;
        let has_state = cache.contains_key(&uri);
        drop(cache);
        
        if !has_state {
            // Compiler state not found, try to rebuild it
            self.client
                .log_message(MessageType::INFO, format!("Compiler state not found for URI, attempting to rebuild: {}", uri))
                .await;
            let documents = self.documents.read().await;
            if let Some(text) = documents.get(&uri) {
                let text_clone = text.clone();
                drop(documents);
                self.rebuild_compiler_state(&uri, &text_clone).await;
            }
        }
        
        let cache = self.compiler_state_cache.read().await;
        if let Some(state) = cache.get(&uri) {
            self.client
                .log_message(MessageType::INFO, format!("Compiler state found for URI, searching for '{}'", identifier))
                .await;
            
            // Use symbol table to find symbol
            let symbols = state.symbols.find_by_name(&identifier);
            if let Some(symbol) = symbols.first() {
                let type_str = hover::format_value_kind(&symbol.ty);
                let hover_content = match symbol.kind {
                    crate::core::hir_lowering::SymbolKind::Function => {
                        // For functions, try to get the full signature from HIR
                        if let Some((func_id, _)) = state.hir.functions.iter()
                            .find(|(_, f)| f.name == identifier) {
                            if let Some(func) = state.hir.functions.get(func_id) {
                                let signature = hover::format_function_signature(func);
                                format!("```cantaloop\n{}\n```", signature)
                            } else {
                                format!("```cantaloop\n{}\n```\nType: `{}`", identifier, type_str)
                            }
                        } else {
                            format!("```cantaloop\n{}\n```\nType: `{}`", identifier, type_str)
                        }
                    }
                    _ => {
                        format!("```cantaloop\n{}\n```\nType: `{}`", identifier, type_str)
                    }
                };
                let range = text_utils::create_range(pos.line as usize, start, end - start);
                return Ok(Some(hover::create_hover_content(hover_content, range)));
            }
            
            self.client
                .log_message(MessageType::INFO, format!("Identifier '{}' not found in symbol table", identifier))
                .await;
        } else {
            self.client
                .log_message(MessageType::WARNING, format!("No compiler state found for URI: {}", uri))
                .await;
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        self.client
            .log_message(MessageType::INFO, format!("Completion requested for URI: {} at line {}", uri, pos.line))
            .await;

        let documents = self.documents.read().await;
        let text = match documents.get(&uri) {
            Some(text) => text,
            None => {
                self.client
                    .log_message(MessageType::WARNING, format!("No document found for URI: {}", uri))
                    .await;
                return Ok(None);
            }
        };

        let lines: Vec<&str> = text.lines().collect();
        if pos.line as usize >= lines.len() {
            return Ok(None);
        }

        let char_pos = pos.character as usize;

        // Get compiler state if available
        let cache = self.compiler_state_cache.read().await;
        let state = cache.get(&uri);
        let response = completion::generate_completions(text, pos.line as usize, char_pos, state);
        drop(cache);

        self.client
            .log_message(MessageType::INFO, format!("Returning completion items"))
            .await;

        Ok(Some(response))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.client
            .log_message(MessageType::INFO, "Semantic tokens requested")
            .await;
        
        let uri = params.text_document.uri;
        let documents = self.documents.read().await;
        let text = match documents.get(&uri) {
            Some(text) => text.clone(),
            None => {
                self.client
                    .log_message(MessageType::WARNING, "No document found for semantic tokens")
                    .await;
                return Ok(None);
            }
        };
        drop(documents);
        
        // Ensure compiler state is built before generating semantic tokens
        let cache = self.compiler_state_cache.read().await;
        let has_state = cache.contains_key(&uri);
        drop(cache);
        
        if !has_state {
            self.client
                .log_message(MessageType::INFO, "Compiler state not found, rebuilding for semantic tokens")
                .await;
            self.rebuild_compiler_state(&uri, &text).await;
        }

        let cache = self.compiler_state_cache.read().await;
        let state = match cache.get(&uri) {
            Some(s) => s,
            None => {
                drop(cache);
                return Ok(None);
            }
        };

        let tokens = semantic_tokens::generate_semantic_tokens(&text, state);
        drop(cache);

        self.client
            .log_message(MessageType::INFO, format!("Returning {} semantic tokens", tokens.len()))
            .await;
        
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }
}
