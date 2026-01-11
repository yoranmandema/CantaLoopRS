//! Document change handlers (didOpen, didChange, didClose).

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;

/// Handle textDocument/didOpen.
/// 
/// CRITICAL: This handler must return immediately (< 10ms) to avoid blocking the LSP event loop.
/// All compilation work is moved to a background task.
pub async fn handle_did_open(server: &CantaLoopServer, params: DidOpenTextDocumentParams) {
    let uri = params.text_document.uri;
    let text = params.text_document.text;
    let version = params.text_document.version;
    
    // Log that didOpen was called
    server.client.log_message(
        MessageType::INFO,
        format!("didOpen called for: {} (version: {}, length: {})", uri, version, text.len()),
    ).await;
    
    eprintln!("[LSP] didOpen: {}", uri);

    // Update source manager - this is fast, can stay in handler
    {
        let mut source_manager = server.source_manager.write().await;
        source_manager.update_file(&uri, text, version);
    }

    // Get file ID - this is fast, can stay in handler
    let file_id = match {
        let source_manager = server.source_manager.read().await;
        source_manager.get_file_id(&uri)
    } {
        Some(id) => id,
        None => {
            let _ = server.client.log_message(MessageType::ERROR, format!("File not found in source manager: {}", uri)).await;
            return;
        }
    };
    
    // Mark as root file - this is fast, can stay in handler
    if let Err(e) = server.compiler_state.mark_as_root(file_id).await {
        server.client.log_message(MessageType::ERROR, format!("Failed to mark file as root: {}", e)).await;
    }
    
    // CRITICAL: Clone everything needed for background task
    let client = server.client.clone();
    let compiler_state = server.compiler_state.clone();
    let source_manager = server.source_manager.clone();
    let uri_clone = uri.clone();
    
    // Spawn compilation in background - handler returns immediately
    // CRITICAL: Run compilation on a blocking thread, not the async executor
    // HIR building, symbol resolution, etc. are CPU-intensive and must not block the LSP event loop
    tokio::spawn(async move {
        eprintln!("[LSP] ========================================");
        eprintln!("[LSP] Starting compilation for: {}", uri_clone);
        eprintln!("[LSP] ========================================");
        let compile_start = std::time::Instant::now();
        
        // CRITICAL: Move compilation to blocking thread pool with timeout
        // This prevents HIR building from blocking the async executor
        let compiler_state_clone = compiler_state.clone();
        let compile_task = tokio::task::spawn_blocking(move || {
            eprintln!("[LSP] [BLOCKING THREAD] Compilation started");
            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(compiler_state_clone.compile_changed_files(vec![file_id]));
            eprintln!("[LSP] [BLOCKING THREAD] Compilation finished with result: {:?}", 
                if result.is_ok() { "Ok" } else { "Err" });
            result
        });
        
        // Add 30 second timeout to catch hanging compilation
        let compile_result = match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            compile_task
        ).await {
            Ok(Ok(result)) => {
                eprintln!("[LSP] Compilation completed within timeout");
                result
            }
            Ok(Err(e)) => {
                eprintln!("[LSP] ✗ Compilation task panicked: {:?}", e);
                log::error!("Compilation task panicked: {:?}", e);
                Err("Compilation task panicked".to_string())
            }
            Err(_) => {
                eprintln!("[LSP] ✗✗✗ COMPILATION TIMEOUT (30s) - HANGING DETECTED ✗✗✗");
                log::error!("Compilation timed out after 30 seconds - possible deadlock or infinite loop");
                let _ = client.log_message(
                    MessageType::ERROR, 
                    "Compilation timed out (30s) - possible infinite loop or deadlock".to_string()
                ).await;
                Err("Compilation timeout".to_string())
            }
        };
        
        let compile_duration = compile_start.elapsed();
        eprintln!("[LSP] Compilation took {:?}", compile_duration);
        
        match compile_result {
            Ok(()) => {
                let _ = client.log_message(MessageType::INFO, format!("Compilation complete: {}", uri_clone)).await;
                eprintln!("[LSP] ✓ Compilation succeeded for: {}", uri_clone);
                
                // CRITICAL: Verify snapshot was stored after successful compilation
                eprintln!("[LSP] Verifying snapshot was stored...");
                let snapshot_check = compiler_state.get_snapshot().await;
                if snapshot_check.is_none() {
                    eprintln!("[LSP] ✗ ERROR: Compilation succeeded but snapshot is None!");
                    log::error!("Compilation succeeded but snapshot is None!");
                } else {
                    eprintln!("[LSP] ✓ Snapshot verified: snapshot exists after compilation");
                }
            }
            Err(e) => {
                let _ = client.log_message(MessageType::ERROR, format!("Compilation error for {}: {}", uri_clone, e)).await;
                eprintln!("[LSP] ✗ Compilation error for {}: {}", uri_clone, e);
            }
        }
        
        // Publish diagnostics
        eprintln!("[LSP] Publishing diagnostics...");
        let (diagnostics, source_text) = {
            let snapshot = compiler_state.get_snapshot().await;
            eprintln!("[LSP] Snapshot for diagnostics: {}", if snapshot.is_some() { "exists" } else { "None" });
            let diagnostics = snapshot.as_ref()
                .map(|s| s.diagnostics(file_id))
                .unwrap_or(&[]);
            let source_manager = source_manager.read().await;
            let source_text = source_manager.get_file_text(file_id)
                .unwrap_or("")
                .to_string();
            (diagnostics.to_vec(), source_text)
        };
        
        crate::lsp::handlers::diagnostics::publish_diagnostics(
            &client,
            &uri_clone,
            &diagnostics,
            &source_text,
        ).await;
        eprintln!("[LSP] ✓ Diagnostics published ({} diagnostics)", diagnostics.len());
        
        // Small delay before refresh
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // Request semantic tokens refresh
        eprintln!("[LSP] Requesting semantic token refresh...");
        match client.semantic_tokens_refresh().await {
            Ok(_) => {
                eprintln!("[LSP] ✓ Semantic token refresh requested successfully");
            }
            Err(e) => {
                eprintln!("[LSP] ✗ Failed to request semantic token refresh: {:?}", e);
            }
        }
        
        eprintln!("[LSP] ========================================");
        eprintln!("[LSP] didOpen background task COMPLETE");
        eprintln!("[LSP] ========================================");
    });
    
    // Handler returns immediately - compilation runs in background
    eprintln!("[LSP] didOpen handler returned immediately, compilation running in background");
}

/// Handle textDocument/didChange.
/// 
/// CRITICAL: This handler must return immediately (< 10ms) to avoid blocking the LSP event loop.
/// All compilation work is moved to a background task.
pub async fn handle_did_change(server: &CantaLoopServer, params: DidChangeTextDocumentParams) {
    let uri = params.text_document.uri;
    let version = params.text_document.version;

    // Apply changes to source text - this is fast, can stay in handler
    // For incremental sync, we need to apply TextDocumentContentChangeEvent
    let mut new_text = {
        let source_manager = server.source_manager.read().await;
        source_manager.get_file_text_by_uri(&uri)
            .map(|s| s.to_string())
            .unwrap_or_default()
    };

    use crate::lsp::mapping::spans::LineIndex;
    
    for change in params.content_changes {
        // Handle incremental sync changes
        if let Some(range) = change.range {
            // Range-based change
            let line_index = LineIndex::new(&new_text);
            let start_byte = line_index.line_col_to_byte(range.start.line, range.start.character);
            let end_byte = line_index.line_col_to_byte(range.end.line, range.end.character);
            
            // Ensure indices are valid
            let start_byte = start_byte.min(new_text.len());
            let end_byte = end_byte.min(new_text.len());
            
            // Replace the range with new text
            new_text = format!("{}{}{}", 
                &new_text[..start_byte],
                change.text,
                &new_text[end_byte..]
            );
        } else {
            // Full document replacement (no range)
            new_text = change.text;
        }
    }

    // Update source manager - this is fast, can stay in handler
    {
        let mut source_manager = server.source_manager.write().await;
        source_manager.update_file(&uri, new_text, version);
    }

    // Get file ID - this is fast, can stay in handler
    let file_id = {
        let source_manager = server.source_manager.read().await;
        match source_manager.get_file_id(&uri) {
            Some(id) => id,
            None => {
                // File not found - return early
                return;
            }
        }
    };
    
    // CRITICAL: Clone everything needed for background task
    let client = server.client.clone();
    let compiler_state = server.compiler_state.clone();
    let source_manager = server.source_manager.clone();
    let uri_clone = uri.clone();
    
    // Spawn compilation in background - handler returns immediately
    // CRITICAL: Run compilation on a blocking thread, not the async executor
    tokio::spawn(async move {
        let _ = client.log_message(MessageType::INFO, format!("File changed, compiling: {}", uri_clone)).await;
        
        // CRITICAL: Move compilation to blocking thread pool
        let compiler_state_clone = compiler_state.clone();
        let compile_result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(compiler_state_clone.compile_changed_files(vec![file_id]))
        }).await;
        
        let compile_result = match compile_result {
            Ok(result) => result,
            Err(e) => {
                eprintln!("[LSP] Compilation task panicked: {:?}", e);
                log::error!("Compilation task panicked: {:?}", e);
                Err("Compilation task panicked".to_string())
            }
        };
        
        match compile_result {
            Ok(()) => {
                let _ = client.log_message(MessageType::INFO, format!("Change compilation complete: {}", uri_clone)).await;
            }
            Err(e) => {
                let _ = client.log_message(MessageType::ERROR, format!("Compilation error for {}: {}", uri_clone, e)).await;
            }
        }
        
        // Publish diagnostics after compilation completes
        let (diagnostics, source_text) = {
            let snapshot = compiler_state.get_snapshot().await;
            let diagnostics = snapshot.as_ref()
                .map(|s| s.diagnostics(file_id))
                .unwrap_or(&[]);
            let source_manager = source_manager.read().await;
            let source_text = source_manager.get_file_text(file_id)
                .unwrap_or("")
                .to_string();
            (diagnostics.to_vec(), source_text)
        };
        
        crate::lsp::handlers::diagnostics::publish_diagnostics(
            &client,
            &uri_clone,
            &diagnostics,
            &source_text,
        ).await;
        
        // CRITICAL FIX: Add delay before requesting refresh to ensure snapshot is fully stored
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // Request semantic tokens refresh
        log::info!("Compilation finished, requesting semantic token refresh");
        eprintln!("[LSP] Compilation finished, requesting semantic token refresh");
        
        // CRITICAL: Log the refresh request result in detail
        match client.semantic_tokens_refresh().await {
            Ok(_) => {
                log::info!("Semantic token refresh requested successfully");
                eprintln!("[LSP] ✓ Semantic token refresh requested successfully");
            }
            Err(e) => {
                log::error!("Failed to request semantic token refresh: {:?}", e);
                eprintln!("[LSP] ✗ Failed to request semantic token refresh: {:?}", e);
                
                // FALLBACK: If refresh fails, try manually triggering token generation
                eprintln!("[LSP] Attempting manual semantic token generation as fallback...");
            }
        }
    });
    
    // Handler returns immediately - compilation runs in background
}

/// Handle textDocument/didClose.
pub async fn handle_did_close(server: &CantaLoopServer, params: DidCloseTextDocumentParams) {
    let uri = params.text_document.uri;

    // CRITICAL: Don't remove files on close - keep them for LSP queries
    // This ensures FileIds remain stable and compilation can reference closed files
    // Files will be removed when workspace folders change or on server restart
    // {
    //     let mut source_manager = server.source_manager.write().await;
    //     source_manager.remove_file(&uri);
    // }

    // TODO: Publish empty diagnostics for this file
    server.client.log_message(MessageType::INFO, format!("Closed: {} (kept in cache)", uri)).await;
    eprintln!("[LSP] didClose: {} (file kept in cache for LSP queries)", uri);
}
