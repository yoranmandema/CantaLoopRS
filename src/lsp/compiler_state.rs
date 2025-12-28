use std::collections::HashMap;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;
use tower_lsp::Client;

use crate::core::engine::Engine;
use crate::core::semantic_analyser::CompilerState;

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

    pub async fn rebuild(&self, uri: &Url, text: &str) {
        // Use the compiler to build state - single source of truth
        match self.engine.compile_for_lsp(text) {
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

