use std::collections::HashMap;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;
use tower_lsp::Client;

use crate::core::engine::Engine;
use crate::core::hir_lowering::CompilerState;

/// Manages compiler state caching and rebuilding for LSP.
pub struct CompilerStateManager {
    cache: Arc<tokio::sync::RwLock<HashMap<Url, CompilerState>>>,
    engine: Arc<Engine>,
    client: Client,
}

impl CompilerStateManager {
    pub fn new(engine: Arc<Engine>, client: Client) -> Self {
        Self {
            cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            engine,
            client,
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

    pub async fn rebuild(&self, uri: &Url, text: &str) {
        // Find project root if this file is part of a project
        let project_root = Self::find_project_root(uri);
        
        // Get the current file path to skip it when loading modules
        let current_file = uri.to_file_path().ok();
        
        // Use the compiler to build state - single source of truth
        match self.engine.compile_for_lsp(text, project_root.as_deref(), current_file.as_deref()) {
            Ok(state) => {
                let mut cache = self.cache.write().await;
                cache.insert(uri.clone(), state);
                self.client
                    .log_message(tower_lsp::lsp_types::MessageType::INFO, format!("Compiler state built successfully for {}", uri))
                    .await;
            }
            Err(e) => {
                // If compilation fails, remove from cache
                let mut cache = self.cache.write().await;
                cache.remove(uri);
                self.client
                    .log_message(tower_lsp::lsp_types::MessageType::WARNING, format!("Compilation failed: {:?}", e))
                    .await;
            }
        }
    }

    pub async fn get(&self, uri: &Url) -> Option<CompilerState> {
        let cache = self.cache.read().await;
        cache.get(uri).cloned()
    }

    pub async fn remove(&self, uri: &Url) {
        let mut cache = self.cache.write().await;
        cache.remove(uri);
    }

    pub fn cache(&self) -> Arc<tokio::sync::RwLock<HashMap<Url, CompilerState>>> {
        self.cache.clone()
    }
}

