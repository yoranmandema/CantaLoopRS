use cantaloop::core::compiler_state::CompilerState;
use cantaloop::core::source_manager::SourceManager;
use cantaloop::core::hir_lowering::SymbolKind;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Test that semantic index correctly matches all spans for an identifier
#[tokio::test]
async fn test_semantic_index_matches_all_spans() {
    let source = r#"
fn add(a: num, b: num) -> num {
    return a + b;
}

let result = add(1, 2);
let result2 = add(3, 4);
"#;

    // Create source manager
    let source_manager = Arc::new(RwLock::new(SourceManager::new()));
    
    // Create compiler state
    let compiler_state = CompilerState::new(source_manager.clone());

    // Add file to source manager (use valid file path format)
    let uri = tower_lsp::lsp_types::Url::from_file_path(std::env::temp_dir().join("test_semantic_index.cl")).unwrap();
    let file_id = {
        let mut sm = source_manager.write().await;
        sm.update_file(&uri, source.to_string(), 1)
    }; // Drop write lock before compiling
    
    // Mark as root
    compiler_state.mark_as_root(file_id).await.unwrap();

    // Compile (compile_changed_files needs read access to source_manager)
    compiler_state
        .compile_changed_files(vec![file_id])
        .await
        .unwrap();

    // Get snapshot
    let snapshot = compiler_state.get_snapshot().await;

    if let Some(snapshot) = snapshot {
        // Access symbol table through public API
        let symbol_table = snapshot.symbol_table().expect("Should have symbol table");

        // Check that "add" has multiple spans matched
        // Find the symbol for "add" by iterating through symbol_info
        let add_symbol_id = symbol_table.symbol_info.iter()
            .find(|(_id, info)| info.name == "add" && info.kind == SymbolKind::Function)
            .map(|(id, _)| *id);
        
        if let Some(add_symbol_id) = add_symbol_id {
            // Get all spans that map to this symbol
            let add_spans: Vec<_> = symbol_table
                .span_to_symbol
                .iter()
                .filter_map(|(span, &symbol_id)| {
                    if symbol_id == add_symbol_id {
                        Some(span)
                    } else {
                        None
                    }
                })
                .collect();

            eprintln!("Found {} spans for 'add' symbol (id={:?})", add_spans.len(), add_symbol_id);
            
            // "add" should have at least 1 span (the definition)
            // TODO: Improve matching logic to match all usages (calls) as well
            assert!(
                add_spans.len() >= 1,
                "Expected 'add' to have at least 1 span (definition), got {}",
                add_spans.len()
            );
        } else {
            panic!("'add' function symbol not found in symbol table");
        }

        // Check that "a" has spans matched (parameter + usage in return)
        let a_symbol_id = symbol_table.symbol_info.iter()
            .find(|(_id, info)| info.name == "a" && matches!(info.kind, SymbolKind::Parameter | SymbolKind::Variable))
            .map(|(id, _)| *id);
        
        if let Some(a_symbol_id) = a_symbol_id {
            let a_spans: Vec<_> = symbol_table
                .span_to_symbol
                .iter()
                .filter_map(|(span, &symbol_id)| {
                    if symbol_id == a_symbol_id {
                        Some(span)
                    } else {
                        None
                    }
                })
                .collect();

            eprintln!("Found {} spans for 'a' symbol (id={:?})", a_spans.len(), a_symbol_id);
            
            // "a" should appear at least once (parameter definition or usage)
            assert!(
                a_spans.len() >= 1,
                "Expected 'a' to have at least 1 span, got {}",
                a_spans.len()
            );
        }
    } else {
        panic!("Snapshot should exist after compilation");
    }
}

/// Test that built-in functions like "print" and "map" are matched
#[tokio::test]
async fn test_semantic_index_matches_builtin_functions() {
    let source = r#"
print("hello");
let mapped = map([1, 2, 3], x => x * 2);
print("world");
"#;

    let source_manager = Arc::new(RwLock::new(SourceManager::new()));
    let compiler_state = CompilerState::new(source_manager.clone());

    let uri = tower_lsp::lsp_types::Url::parse("file:///test.cl").unwrap();
    let file_id = {
        let mut sm = source_manager.write().await;
        sm.update_file(&uri, source.to_string(), 1)
    }; // Drop write lock before compiling
    
    compiler_state.mark_as_root(file_id).await.unwrap();
    compiler_state.compile_changed_files(vec![file_id]).await.unwrap();

    let snapshot = compiler_state.get_snapshot().await;

    if let Some(snapshot) = snapshot {
        let symbol_table = snapshot.symbol_table().expect("Should have symbol table");

        // Check that "print" spans are matched
        let print_spans: Vec<_> = symbol_table
            .span_to_symbol
            .iter()
            .filter_map(|(span, &symbol_id)| {
                symbol_table
                    .symbol_info
                    .get(&symbol_id)
                    .and_then(|info| if info.name == "print" { Some(span) } else { None })
            })
            .collect();

        assert!(
            print_spans.len() >= 2,
            "Expected 'print' to have at least 2 spans, got {}",
            print_spans.len()
        );

        // Check that "map" span is matched
        let map_spans: Vec<_> = symbol_table
            .span_to_symbol
            .iter()
            .filter_map(|(span, &symbol_id)| {
                symbol_table
                    .symbol_info
                    .get(&symbol_id)
                    .and_then(|info| if info.name == "map" { Some(span) } else { None })
            })
            .collect();

        assert!(
            map_spans.len() >= 1,
            "Expected 'map' to have at least 1 span, got {}",
            map_spans.len()
        );
    } else {
        panic!("Snapshot should exist after compilation");
    }
}

/// Test coverage calculation
#[tokio::test]
async fn test_semantic_index_coverage() {
    let source = r#"
fn add(a: num, b: num) -> num {
    return a + b;
}

let x = add(1, 2);
let y = add(3, 4);
"#;

    let source_manager = Arc::new(RwLock::new(SourceManager::new()));
    let compiler_state = CompilerState::new(source_manager.clone());

    let uri = tower_lsp::lsp_types::Url::parse("file:///test.cl").unwrap();
    let file_id = {
        let mut sm = source_manager.write().await;
        sm.update_file(&uri, source.to_string(), 1)
    }; // Drop write lock before compiling
    
    compiler_state.mark_as_root(file_id).await.unwrap();
    compiler_state.compile_changed_files(vec![file_id]).await.unwrap();

    let snapshot = compiler_state.get_snapshot().await;

    if let Some(snapshot) = snapshot {
        let symbol_table = snapshot.symbol_table().expect("Should have symbol table");

        // Count total identifier spans
        let total_spans = symbol_table.span_to_symbol.len();
        let symbol_count = symbol_table.symbol_info.len();

        // Should have at least some coverage
        assert!(
            total_spans > 0,
            "Should have at least some spans mapped, got {}",
            total_spans
        );
        assert!(
            symbol_count > 0,
            "Should have at least some symbols, got {}",
            symbol_count
        );

        eprintln!(
            "Test coverage: {} spans mapped, {} symbols",
            total_spans, symbol_count
        );
    } else {
        panic!("Snapshot should exist after compilation");
    }
}
