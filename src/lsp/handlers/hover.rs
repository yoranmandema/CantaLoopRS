//! Hover handler.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;
use crate::lsp::mapping::spans::position_to_byte_offset;
use crate::core::hir_lowering::SymbolId;
use crate::core::hir_lowering::ValueKind;

/// Handle textDocument/hover.
pub async fn handle_hover(
    server: &CantaLoopServer,
    params: HoverParams,
) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // Get source text and file ID
    let (file_id, source_text) = {
        let source_manager = server.source_manager.read().await;
        let file_id = match source_manager.get_file_id(uri) {
            Some(id) => id,
            None => return Ok(None),
        };
        let text = source_manager.get_file_text(file_id)
            .unwrap_or("")
            .to_string();
        (file_id, text)
    };

    // Convert LSP position to byte offset
    let byte_offset = position_to_byte_offset(position, &source_text);

    // Get compiler snapshot for this file
    // Use last good snapshot if current compilation failed
    let snapshot = match server.compiler_state.get_snapshot_for_file(file_id).await {
        Some(s) => s,
        None => return Ok(None),
    };
    
    // CRITICAL: Only provide hover if file parsed successfully
    // Check if CST exists for this file - if not, file failed to parse
    if snapshot.cst(file_id).is_none() {
        // File failed to parse - return None (no hover data available)
        return Ok(None);
    }

    // Find symbol at this position
    // Uses span-based lookup which includes both definitions and references
    let symbols_at_pos: Vec<_> = snapshot.symbols_at_offset(file_id, byte_offset).collect();
    eprintln!("[HOVER] Found {} symbol(s) at byte {}:", symbols_at_pos.len(), byte_offset);
    for (span, symbol_id) in &symbols_at_pos {
        if let Some(info) = snapshot.symbol_info(*symbol_id) {
            // Check if this is a definition or reference
            let is_definition = snapshot.definition_span_for_symbol(*symbol_id)
                .map(|def_span| def_span.start == span.start && def_span.end == span.end)
                .unwrap_or(false);
            let role = if is_definition { "definition" } else { "reference" };
            eprintln!("[HOVER]   - span={}..{}, SymbolId({}), name='{}', kind={:?}, role={}", 
                span.start, span.end, symbol_id.0, info.name, info.kind, role);
        }
    }
    // Prefer the most plausible symbol: its span slice should contain its own name.
    // This mitigates rare cases where cross-file spans leak into the snapshot and happen
    // to overlap the hovered byte offset.
    let symbol_id = symbols_at_pos
        .iter()
        .find_map(|(span, symbol_id)| {
            let info = snapshot.symbol_info(*symbol_id)?;
            let base_name = info.name.split('.').last().unwrap_or(info.name.as_str());
            if span.end <= source_text.len() && source_text[span.start..span.end].contains(base_name) {
                eprintln!(
                    "[HOVER] Using best-match symbol: SymbolId({}), name='{}', kind={:?}",
                    symbol_id.0, info.name, info.kind
                );
                Some(*symbol_id)
            } else {
                None
            }
        })
        .or_else(|| {
            symbols_at_pos.first().map(|(_, symbol_id)| {
                if let Some(info) = snapshot.symbol_info(*symbol_id) {
                    eprintln!(
                        "[HOVER] Using first symbol (fallback): SymbolId({}), name='{}', kind={:?}",
                        symbol_id.0, info.name, info.kind
                    );
                }
                *symbol_id
            })
        });

    // Build hover content from symbol information
    let hover_content = match symbol_id {
        Some(sym_id) => {
            build_hover_content(sym_id, &snapshot)
        }
        None => None,
    };

    match hover_content {
        Some(content) => Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: None, // Could optionally provide range highlighting
        })),
        None => {
            // Fallback: struct field hover (e.g. `final.iter`, `state.iter`).
            if let Some(content) = hover_field_access(&snapshot, file_id, &source_text, byte_offset) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: content,
                    }),
                    range: None,
                }));
            }

            // Fallback #1: if we can identify a word under the cursor, but spans were not recorded
            // (e.g. partial compilation), try a name-based lookup in the symbol table.
            if let Some(symbols) = snapshot.symbol_table() {
                if let Some(word) = extract_word_at_byte(&source_text, byte_offset) {
                    if let Some((sid, _)) = symbols
                        .symbol_info
                        .iter()
                        .find(|(_, info)| info.name == word)
                    {
                        if let Some(content) = build_hover_content(*sid, &snapshot) {
                            return Ok(Some(Hover {
                                contents: HoverContents::Markup(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: content,
                                }),
                                range: None,
                            }));
                        }
                    }
                }
            }

            // Fallback: hover for user-defined struct types in type annotations.
            // These often appear in CST as plain spans (not identifiers lowered into HIR),
            // so they won't be present in the symbol table.
            if let Some(hir) = snapshot.hir() {
                if let Some(word) = extract_word_at_byte(&source_text, byte_offset) {
                    if let Some(def) = hir.structs.get(&word) {
                        let mut lines = Vec::new();
                        lines.push(format!("**struct** `{}`", def.name));
                        if !def.fields.is_empty() {
                            lines.push("```cantaloop".to_string());
                            lines.push(format!("struct {} {{", def.name));
                            for (fname, fty) in &def.fields {
                                lines.push(format!("  {}: {}", fname, format_value_kind(fty)));
                            }
                            lines.push("}".to_string());
                            lines.push("```".to_string());
                        }
                        return Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: lines.join("\n"),
                            }),
                            range: None,
                        }));
                    }
                }
            }

            Ok(None)
        }
    }
}

fn hover_field_access(
    snapshot: &crate::core::lsp_api::CompilerSnapshot,
    file_id: crate::core::source_manager::FileId,
    source: &str,
    byte_offset: usize,
) -> Option<String> {
    use crate::core::cst::{CstCallArgument, CstClosureBody, CstExpr, CstStatement};

    let cst = snapshot.cst(file_id)?;
    let hir = snapshot.hir()?;

    fn struct_name_of_ident(
        snapshot: &crate::core::lsp_api::CompilerSnapshot,
        file_id: crate::core::source_manager::FileId,
        source: &str,
        ident: &crate::core::cst::Spanned<String>,
    ) -> Option<String> {
        let name = ident.node.as_str();
        let ident_off = ident.span.start as usize;
        let mut candidates: Vec<_> = snapshot.symbols_at_offset(file_id, ident_off).collect();
        // Prefer the most specific match whose slice equals the identifier.
        candidates.sort_by_key(|(sp, _)| sp.end - sp.start);
        for (sp, sid) in candidates {
            if sp.end <= source.len() && &source[sp.start..sp.end] == name {
                let info = snapshot.symbol_info(sid)?;
                if let ValueKind::Struct(s) = &info.ty {
                    return Some(s.clone());
                }
            }
        }
        None
    }

    fn find_field_hover(
        snapshot: &crate::core::lsp_api::CompilerSnapshot,
        file_id: crate::core::source_manager::FileId,
        source: &str,
        expr: &crate::core::cst::Spanned<CstExpr>,
        byte_offset: usize,
    ) -> Option<(String, String)> {
        let sp = expr.span;
        if !(sp.start as usize <= byte_offset && byte_offset < sp.end as usize) {
            return None;
        }

        match &expr.node {
            CstExpr::FieldAccess { object, field, .. } => {
                if field.span.start as usize <= byte_offset && byte_offset < field.span.end as usize {
                    if let CstExpr::Identifier(obj) = &object.node {
                        let struct_name = struct_name_of_ident(snapshot, file_id, source, obj)?;
                        return Some((struct_name, field.node.clone()));
                    }
                    return None;
                }
                find_field_hover(snapshot, file_id, source, object, byte_offset)
            }
            CstExpr::MemberAccess { object, members, .. } => {
                // MemberAccess can represent modules too; only treat it as a struct-field hover if
                // the base identifier is a struct-typed variable.
                if let CstExpr::Identifier(obj) = &object.node {
                    if let Some(struct_name) = struct_name_of_ident(snapshot, file_id, source, obj) {
                        for m in members {
                            if m.span.start as usize <= byte_offset && byte_offset < m.span.end as usize {
                                return Some((struct_name, m.node.clone()));
                            }
                        }
                    }
                }
                // Recurse for nested cases.
                if let Some(v) = find_field_hover(snapshot, file_id, source, object, byte_offset) {
                    return Some(v);
                }
                for m in members {
                    let _ = m;
                }
                None
            }
            CstExpr::FunctionCall { callee, arguments, .. } => {
                find_field_hover(snapshot, file_id, source, callee, byte_offset).or_else(|| {
                    for a in arguments {
                        if let CstCallArgument::Expr(e) = &a.node {
                            if let Some(v) = find_field_hover(snapshot, file_id, source, e, byte_offset) {
                                return Some(v);
                            }
                        }
                    }
                    None
                })
            }
            CstExpr::PartialCall { func, args, .. } => {
                find_field_hover(snapshot, file_id, source, func, byte_offset).or_else(|| {
                    for a in args {
                        if let CstCallArgument::Expr(e) = &a.node {
                            if let Some(v) = find_field_hover(snapshot, file_id, source, e, byte_offset) {
                                return Some(v);
                            }
                        }
                    }
                    None
                })
            }
            CstExpr::Infix { lhs, rhs, .. } | CstExpr::Compose { lhs, rhs, .. } => {
                find_field_hover(snapshot, file_id, source, lhs, byte_offset)
                    .or_else(|| find_field_hover(snapshot, file_id, source, rhs, byte_offset))
            }
            CstExpr::Prefix { rhs, .. } => find_field_hover(snapshot, file_id, source, rhs, byte_offset),
            CstExpr::Postfix { lhs, .. } => find_field_hover(snapshot, file_id, source, lhs, byte_offset),
            CstExpr::Group { inner, .. } => find_field_hover(snapshot, file_id, source, inner, byte_offset),
            CstExpr::Array { elements, .. } => {
                for e in elements {
                    if let Some(v) = find_field_hover(snapshot, file_id, source, e, byte_offset) {
                        return Some(v);
                    }
                }
                None
            }
            CstExpr::ArrayIndex { array, indices, .. } => {
                if let Some(v) = find_field_hover(snapshot, file_id, source, array, byte_offset) {
                    return Some(v);
                }
                for idx in indices {
                    match &idx.node {
                        crate::core::cst::CstIndexSpec::Single(e) => {
                            if let Some(v) = find_field_hover(snapshot, file_id, source, e, byte_offset) {
                                return Some(v);
                            }
                        }
                        crate::core::cst::CstIndexSpec::Range { start, end, .. }
                        | crate::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                            if let Some(s) = start {
                                if let Some(v) = find_field_hover(snapshot, file_id, source, s, byte_offset) {
                                    return Some(v);
                                }
                            }
                            if let Some(e) = end {
                                if let Some(v) = find_field_hover(snapshot, file_id, source, e, byte_offset) {
                                    return Some(v);
                                }
                            }
                        }
                    }
                }
                None
            }
            CstExpr::Closure { body, .. } => match body {
                CstClosureBody::Expression(e) => find_field_hover(snapshot, file_id, source, e, byte_offset),
                CstClosureBody::Block(b) => {
                    for st in &b.node.statements {
                        if let Some(v) = find_field_hover_in_stmt(snapshot, file_id, source, st, byte_offset) {
                            return Some(v);
                        }
                    }
                    None
                }
            },
            _ => None,
        }
    }

    fn find_field_hover_in_stmt(
        snapshot: &crate::core::lsp_api::CompilerSnapshot,
        file_id: crate::core::source_manager::FileId,
        source: &str,
        stmt: &crate::core::cst::Spanned<CstStatement>,
        byte_offset: usize,
    ) -> Option<(String, String)> {
        let sp = stmt.span;
        if !(sp.start as usize <= byte_offset && byte_offset < sp.end as usize) {
            return None;
        }
        match &stmt.node {
            CstStatement::Let { expression, .. }
            | CstStatement::Const { expression, .. }
            | CstStatement::Assign { expression, .. } => find_field_hover(snapshot, file_id, source, expression, byte_offset),
            CstStatement::Expression(e) => find_field_hover(snapshot, file_id, source, e, byte_offset),
            CstStatement::Return { expression, .. } => find_field_hover(snapshot, file_id, source, expression, byte_offset),
            CstStatement::If { arms, else_block, .. } => {
                for (cond, blk) in arms {
                    if let Some(v) = find_field_hover(snapshot, file_id, source, cond, byte_offset) {
                        return Some(v);
                    }
                    for st in &blk.node.statements {
                        if let Some(v) = find_field_hover_in_stmt(snapshot, file_id, source, st, byte_offset) {
                            return Some(v);
                        }
                    }
                }
                if let Some(eb) = else_block {
                    for st in &eb.node.statements {
                        if let Some(v) = find_field_hover_in_stmt(snapshot, file_id, source, st, byte_offset) {
                            return Some(v);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    // Find the most specific field access under the cursor.
    let mut hit: Option<(String, String)> = None;
    for block in &cst.blocks {
        if !(block.span.start as usize <= byte_offset && byte_offset < block.span.end as usize) {
            continue;
        }
        for stmt in &block.node.statements {
            if let Some(v) = find_field_hover_in_stmt(snapshot, file_id, source, stmt, byte_offset) {
                hit = Some(v);
                break;
            }
        }
        if hit.is_some() {
            break;
        }
    }

    let (struct_name, field_name) = hit?;
    let def = hir.structs.get(&struct_name)?;
    let field_ty = def
        .fields
        .iter()
        .find(|(n, _)| n == &field_name)
        .map(|(_, ty)| ty)?;

    let mut lines = Vec::new();
    lines.push(format!("**field** `{}`", field_name));
    lines.push("```cantaloop".to_string());
    lines.push(format!("{}: {}", field_name, format_value_kind(field_ty)));
    lines.push("```".to_string());
    lines.push(format!("**of struct** `{}`", struct_name));
    Some(lines.join("\n"))
}

fn extract_word_at_byte(source: &str, byte: usize) -> Option<String> {
    if byte >= source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    let is_ident = |b: u8| (b as char).is_ascii_alphanumeric() || b == b'_';

    if !is_ident(bytes[byte]) {
        return None;
    }

    let mut start = byte;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = byte;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }
    Some(source[start..end].to_string())
}

/// Build hover content from symbol information.
fn build_hover_content(symbol_id: SymbolId, snapshot: &crate::core::lsp_api::CompilerSnapshot) -> Option<String> {
    // Get symbol info from snapshot
    let info = snapshot.symbol_info(symbol_id)?;
    let hir = snapshot.hir()?;
    
    eprintln!("[HOVER] build_hover_content: SymbolId({}), name='{}', kind={:?}", 
        symbol_id.0, info.name, info.kind);
    
    // Build hover content based on symbol kind and type
    let mut parts = Vec::new();
    
    match &info.kind {
        crate::core::hir_lowering::SymbolKind::Function => {
            // For functions, use EntityId for direct lookup (no name-based search!)
            let found_func = if let Some(entity_id) = info.entity_id {
                eprintln!("[HOVER] Looking up function by EntityId: {:?}", entity_id);
                hir.functions.get(&entity_id)
            } else {
                eprintln!("[HOVER] WARNING: Function symbol '{}' has no entity_id, falling back to name search", info.name);
                // Fallback: search by name (should rarely happen)
                hir.functions.values().find(|func| func.name == info.name)
            };

            if let Some(func) = found_func {
                eprintln!("[HOVER] Found function: EntityId={:?}, name='{}'", func.id, func.name);
                    // Format function signature
                    let arrow = if func.signature.is_effectful { "~>" } else { "->" };
                    let params_str = if func.signature.params.is_empty() {
                        "()".to_string()
                    } else {
                        format!("({})", func.signature.params.iter()
                            .map(|p| format_value_kind(p))
                            .collect::<Vec<_>>()
                            .join(", "))
                    };
                    let return_str = format_value_kind(&func.signature.return_type);
                    
                    parts.push(format!("**function** `{}`", info.name));
                    parts.push(format!("```cantaloop\nfn {}{} {} {}\n```", 
                        info.name, params_str, arrow, return_str));
                    
                    // Add effect information
                    if func.signature.is_effectful {
                        parts.push("\n*Effectful function* — requires execution marker (`!`)".to_string());
                    } else {
                        parts.push("\n*Pure function* — no side effects".to_string());
                    }
                    
                    return Some(parts.join("\n\n"));
                } else {
                    eprintln!("[HOVER] WARNING: No matching function found in HIR for name '{}'", info.name);
                }
            
            // Fallback: use type info
            parts.push(format!("**function** `{}`", info.name));
            parts.push(format!("```cantaloop\n{}\n```", format_value_kind(&info.ty)));
        }
        crate::core::hir_lowering::SymbolKind::Variable => {
            parts.push(format!("**variable** `{}`", info.name));
            parts.push(format!("```cantaloop\n{}\n```", format_value_kind(&info.ty)));
        }
        crate::core::hir_lowering::SymbolKind::Parameter => {
            parts.push(format!("**parameter** `{}`", info.name));
            parts.push(format!("```cantaloop\n{}\n```", format_value_kind(&info.ty)));
        }
        crate::core::hir_lowering::SymbolKind::Field => {
            // Prefer showing the base field name (`iter`) while keeping struct context in the label.
            let base = info.name.split('.').last().unwrap_or(info.name.as_str());
            parts.push(format!("**field** `{}`", base));
            parts.push(format!("```cantaloop\n{}: {}\n```", base, format_value_kind(&info.ty)));
            if let Some(struct_name) = info.name.split('.').next() {
                if struct_name != base {
                    parts.push(format!("**of struct** `{}`", struct_name));
                }
            }
        }
        crate::core::hir_lowering::SymbolKind::Module => {
            parts.push(format!("**module** `{}`", info.name));
        }
        crate::core::hir_lowering::SymbolKind::Type => {
            parts.push(format!("**type** `{}`", info.name));
            parts.push(format!("```cantaloop\n{}\n```", format_value_kind(&info.ty)));
        }
    }
    
    Some(parts.join("\n\n"))
}

/// Format ValueKind as a readable type string.
fn format_value_kind(kind: &crate::core::hir_lowering::ValueKind) -> String {
    match kind {
        crate::core::hir_lowering::ValueKind::Number => "num".to_string(),
        crate::core::hir_lowering::ValueKind::String => "string".to_string(),
        crate::core::hir_lowering::ValueKind::Boolean => "bool".to_string(),
        crate::core::hir_lowering::ValueKind::Any => "any".to_string(),
        crate::core::hir_lowering::ValueKind::Unknown => "unknown".to_string(),
        crate::core::hir_lowering::ValueKind::Void => "void".to_string(),
        crate::core::hir_lowering::ValueKind::TypeVar(id) => format!("T{}", id),
        crate::core::hir_lowering::ValueKind::Function(sig) => sig.clone(),
        crate::core::hir_lowering::ValueKind::Thunk(sig) => sig.clone(),
        crate::core::hir_lowering::ValueKind::FnSig { params, return_type, is_effectful } => {
            let p: Vec<String> = params.iter().map(format_value_kind).collect();
            let param_str = if p.is_empty() {
                "()".to_string()
            } else if p.len() == 1 {
                p[0].clone()
            } else {
                format!("({})", p.join(","))
            };
            let arrow = if *is_effectful { "~>" } else { "->" };
            format!("{} {} {}", param_str, arrow, format_value_kind(return_type))
        }
        crate::core::hir_lowering::ValueKind::ThunkSig { params, return_type, is_effectful } => {
            let p: Vec<String> = params.iter().map(format_value_kind).collect();
            let param_str = if p.is_empty() {
                "()".to_string()
            } else if p.len() == 1 {
                p[0].clone()
            } else {
                format!("({})", p.join(","))
            };
            let arrow = if *is_effectful { "~>" } else { "->" };
            format!("{} {} {}", param_str, arrow, format_value_kind(return_type))
        }
        crate::core::hir_lowering::ValueKind::Callable => "callable".to_string(),
        crate::core::hir_lowering::ValueKind::Array(inner) => {
            format!("{}[]", format_value_kind(inner))
        }
        crate::core::hir_lowering::ValueKind::Struct(name) => name.clone(),
    }
}
