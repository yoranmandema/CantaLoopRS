use cantaloop::core::compiler_state::CompilerState;
use cantaloop::core::hir_lowering::SymbolKind;
use cantaloop::core::source_manager::{FileId, SourceManager};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Url;

async fn compile_fixture() -> (cantaloop::core::lsp_api::CompilerSnapshot, FileId, String) {
    let source = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl")
        .expect("read mandelbrot fixture");

    let source_manager = Arc::new(RwLock::new(SourceManager::new()));
    let compiler_state = CompilerState::new(source_manager.clone());

    let uri = Url::from_file_path(std::env::temp_dir().join("test_field_symbols_mandelbrot.cl")).unwrap();
    let file_id = {
        let mut sm = source_manager.write().await;
        sm.update_file(&uri, source.clone(), 1)
    };

    compiler_state.mark_as_root(file_id).await.unwrap();
    compiler_state.compile_changed_files(vec![file_id]).await.unwrap();

    let snapshot = compiler_state
        .get_snapshot()
        .await
        .expect("Should have snapshot after compilation");

    (snapshot, file_id, source)
}

#[tokio::test]
async fn mandelbrot_iter_field_is_a_symbol_with_type() {
    let (snapshot, file_id, source) = compile_fixture().await;

    let diags = snapshot.diagnostics(file_id);
    assert!(
        diags.is_empty(),
        "expected fixture to compile without diagnostics, got: {:?}",
        diags
    );

    // Sanity: we should have at least one Field symbol in the table.
    let symbols = snapshot.symbol_table().expect("symbol table");
    let field_count = symbols
        .symbol_info
        .values()
        .filter(|info| info.kind == SymbolKind::Field)
        .count();
    assert!(field_count > 0, "expected at least one Field symbol, got 0 (total symbols={})", symbols.symbol_info.len());

    // Hover position on `iter` in `final.iter`
    let off = source.find("final.iter").expect("find final.iter");
    let iter_off = off + "final.".len();
    let final_off = off;

    // Sanity: `final` should resolve to a struct-typed variable.
    let final_syms: Vec<_> = snapshot.symbols_at_offset(file_id, final_off).collect();
    assert!(!final_syms.is_empty(), "expected symbol at final offset");
    let (_, final_sid) = final_syms[0];
    let final_info = snapshot.symbol_info(final_sid).expect("final symbol info");
    assert!(
        matches!(final_info.ty, cantaloop::core::hir_lowering::ValueKind::Struct(_)),
        "expected `final` to be struct-typed, got {:?}",
        final_info.ty
    );

    // Ensure `state` inside the fold closure is contextually typed as a struct (needed for field hover).
    let state_base_off = source.find("state.escaped").expect("find state.escaped");
    let state_base_syms: Vec<_> = snapshot.symbols_at_offset(file_id, state_base_off).collect();
    assert!(!state_base_syms.is_empty(), "expected symbol at state base offset");
    let (_, state_sid) = state_base_syms[0];
    let state_info = snapshot.symbol_info(state_sid).expect("state symbol info");
    assert!(
        matches!(state_info.ty, cantaloop::core::hir_lowering::ValueKind::Struct(_)),
        "expected `state` to be struct-typed via contextual typing, got {:?}",
        state_info.ty
    );

    // Debug: ensure the `State.iter` symbol has a reference span near this offset.
    let iter_sid = symbols
        .symbol_info
        .iter()
        .find(|(_, info)| info.kind == SymbolKind::Field && info.name.ends_with(".iter"))
        .map(|(sid, _)| *sid)
        .expect("find State.iter field symbol id");
    let refs = snapshot.spans_for_symbol(iter_sid).expect("spans for iter field");
    assert!(
        refs.iter().any(|sp| sp.start <= iter_off && iter_off < sp.end),
        "expected some iter-field span to contain iter_off={iter_off}, got spans={refs:?}"
    );

    let syms: Vec<_> = snapshot.symbols_at_offset(file_id, iter_off).collect();
    assert!(!syms.is_empty(), "expected symbols at iter offset");

    let (_, sid) = syms[0];
    let info = snapshot.symbol_info(sid).expect("symbol info");
    assert_eq!(info.kind, SymbolKind::Field, "expected iter to be a Field symbol");
    assert!(
        matches!(info.ty, cantaloop::core::hir_lowering::ValueKind::Number),
        "expected iter field type to be num, got {:?}",
        info.ty
    );
}

