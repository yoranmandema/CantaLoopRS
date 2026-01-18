use cantaloop::core::compiler_state::CompilerState;
use cantaloop::core::source_manager::SourceManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Url;

#[tokio::test]
async fn function_call_argument_types_are_checked() {
    let source = r#"
fn id(x: num) -> num {
  return x;
}

id("hi");
"#
    .to_string();

    let source_manager = Arc::new(RwLock::new(SourceManager::new()));
    let compiler_state = CompilerState::new(source_manager.clone());

    let uri = Url::from_file_path(std::env::temp_dir().join("test_function_call_typechecking.cl")).unwrap();
    let file_id = {
        let mut sm = source_manager.write().await;
        sm.update_file(&uri, source.clone(), 1)
    };

    compiler_state.mark_as_root(file_id).await.unwrap();
    compiler_state.compile_changed_files(vec![file_id]).await.unwrap();
    let snapshot = compiler_state.get_snapshot().await.expect("snapshot");

    let diags = snapshot.diagnostics(file_id);
    assert!(
        diags.iter().any(|d| format!("{:?}", d).contains("Type mismatch in argument 1")),
        "expected type mismatch diagnostic, got: {:?}",
        diags
    );
}

