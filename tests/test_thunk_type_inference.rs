use cantaloop::core::compiler_state::CompilerState;
use cantaloop::core::hir_lowering::ValueKind;
use cantaloop::core::source_manager::{FileId, SourceManager};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Url;

async fn compile_source(source: &str) -> (cantaloop::core::lsp_api::CompilerSnapshot, Url, FileId) {
    let source_manager = Arc::new(RwLock::new(SourceManager::new()));
    let compiler_state = CompilerState::new(source_manager.clone());

    let uri = Url::from_file_path(std::env::temp_dir().join("test_thunk_type_inference.cl")).unwrap();
    let file_id = {
        let mut sm = source_manager.write().await;
        sm.update_file(&uri, source.to_string(), 1)
    };

    compiler_state.mark_as_root(file_id).await.unwrap();
    compiler_state.compile_changed_files(vec![file_id]).await.unwrap();

    let snapshot = compiler_state
        .get_snapshot()
        .await
        .expect("Should have snapshot after compilation");

    (snapshot, uri, file_id)
}

fn get_root_var_kind(snapshot: &cantaloop::core::lsp_api::CompilerSnapshot, name: &str) -> Option<ValueKind> {
    let hir = snapshot.hir()?;
    let root = hir.scopes.scopes.get(0)?;
    root.vars.iter().find(|v| v.name == name).map(|v| v.kind.clone())
}

#[tokio::test]
async fn mandelbrot_partial_call_scale_infers_num_to_num_thunks_and_removes_mandel_iter_type_mismatch() {
    let source = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
    let (snapshot, _uri, file_id) = compile_source(&source).await;

    // The original regression: `mandel_iter(sx, sy)` complained expected num, got unknown.
    let diags = snapshot.diagnostics(file_id);
    let has_mismatch = diags
        .iter()
        .any(|d| format!("{d}").contains("Type mismatch in argument 1 of function 'mandel_iter'"));
    assert!(
        !has_mismatch,
        "unexpected mandel_iter type mismatch diagnostic still present: {:?}",
        diags.iter().map(|d| format!("{d}")).collect::<Vec<_>>()
    );

    // Ensure partial application produces the expected thunk types.
    let scale_x_kind = get_root_var_kind(&snapshot, "scale_x").expect("scale_x var kind");
    let scale_y_kind = get_root_var_kind(&snapshot, "scale_y").expect("scale_y var kind");

    match scale_x_kind {
        ValueKind::Thunk(ref s) => assert!(
            s.contains("num") && s.contains("->") && s.ends_with("num"),
            "expected scale_x thunk like 'num -> num', got {s:?}"
        ),
        other => panic!("expected scale_x to be a thunk, got {:?}", other),
    }

    match scale_y_kind {
        ValueKind::Thunk(ref s) => assert!(
            s.contains("num") && s.contains("->") && s.ends_with("num"),
            "expected scale_y thunk like 'num -> num', got {s:?}"
        ),
        other => panic!("expected scale_y to be a thunk, got {:?}", other),
    }

    // Canonicalization: we should not leak `number` in type strings; use `num`.
    if let ValueKind::Thunk(ref s) = scale_x_kind {
        assert!(!s.contains("number"), "expected canonical type name `num`, got {s:?}");
    }
    if let ValueKind::Thunk(ref s) = scale_y_kind {
        assert!(!s.contains("number"), "expected canonical type name `num`, got {s:?}");
    }
}

