//! Diagnostics handler.
//!
//! Converts compiler diagnostics to LSP diagnostics and publishes them.

use tower_lsp::lsp_types::*;

use crate::core::hir_lowering::HirError;
use crate::lsp::mapping::spans;

/// Publish diagnostics for a file.
pub async fn publish_diagnostics(
    client: &tower_lsp::Client,
    uri: &Url,
    diagnostics: &[HirError],
    source_text: &str,
) {
    let lsp_diagnostics: Vec<Diagnostic> = diagnostics
        .iter()
        .filter_map(|err| {
            map_diagnostic(err, source_text)
        })
        .collect();

    client.publish_diagnostics(uri.clone(), lsp_diagnostics, None).await;
}

/// Convert a compiler diagnostic to an LSP diagnostic.
fn map_diagnostic(error: &HirError, source_text: &str) -> Option<Diagnostic> {
    let span = error.span();
    
    // Convert HIR span (usize) to LSP range
    let range = spans::hir_span_to_range(span, source_text)?;
    
    let severity = match error {
        HirError::NotImplemented { .. } => DiagnosticSeverity::WARNING,
        HirError::UnknownVariable { .. } => DiagnosticSeverity::ERROR,
        HirError::VariableAlreadyDeclared { .. } => DiagnosticSeverity::ERROR,
        HirError::TypeMismatch { .. } => DiagnosticSeverity::ERROR,
        HirError::TypeError { .. } => DiagnosticSeverity::ERROR,
        HirError::BinaryOpTypeError { .. } => DiagnosticSeverity::ERROR,
        HirError::MemberNotFound { .. } => DiagnosticSeverity::ERROR,
        HirError::FunctionNotFound { .. } => DiagnosticSeverity::ERROR,
        HirError::ModuleNotFound { .. } => DiagnosticSeverity::ERROR,
    };

    Some(Diagnostic {
        range,
        severity: Some(severity),
        code: None,
        code_description: None,
        source: Some("cantaloop".to_string()),
        message: error.to_string(),
        related_information: None,
        tags: None,
        data: None,
    })
}
