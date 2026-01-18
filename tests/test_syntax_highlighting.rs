/// Tests for syntax highlighting / semantic token generation
///
/// These tests verify that all identifiers in the source code are properly
/// highlighted by checking that they have corresponding symbol entries.

use cantaloop::core::compiler_state::CompilerState;
use cantaloop::core::source_manager::{FileId, SourceManager};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::Url;

/// Helper to compile source and return snapshot
async fn compile_source(source: &str) -> (cantaloop::core::lsp_api::CompilerSnapshot, Url, FileId) {
    let source_manager = Arc::new(RwLock::new(SourceManager::new()));
    let compiler = CompilerState::new(source_manager.clone());

    let uri = Url::from_file_path(std::env::temp_dir().join("test_syntax.cl")).unwrap();
    let file_id = {
        let mut sm = source_manager.write().await;
        sm.update_file(&uri, source.to_string(), 1)
    };

    compiler.mark_as_root(file_id).await.unwrap();
    compiler.compile_changed_files(vec![file_id]).await.unwrap();

    let snapshot = compiler.get_snapshot().await
        .expect("Should have snapshot after compilation");

    (snapshot, uri, file_id)
}

/// Find all identifier-like tokens in source code
fn extract_identifiers(source: &str) -> Vec<(String, usize, usize)> {
    let mut identifiers = Vec::new();
    let mut chars = source.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        if ch.is_alphabetic() || ch == '_' {
            let mut end = start + ch.len_utf8();
            let mut name = ch.to_string();

            while let Some(&(next_pos, next_ch)) = chars.peek() {
                if next_ch.is_alphanumeric() || next_ch == '_' {
                    name.push(next_ch);
                    end = next_pos + next_ch.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }

            // Skip keywords and literals
            if !matches!(name.as_str(),
                "fn" | "let" | "const" | "if" | "else" | "while" | "for" | "loop" |
                "match" | "return" | "break" | "use" | "from" | "mod" | "struct" |
                "num" | "str" | "bool" | "void" | "true" | "false" | "as" | "any"
            ) {
                identifiers.push((name, start, end));
            }
        }
    }

    identifiers
}

#[tokio::test]
async fn test_use_statement_highlighting() {
    let source = r#"
use print from std;
use map from functional;

fn test() -> num {
    return 42;
}
"#;

    let (snapshot, _uri, _file_id) = compile_source(source).await;
    let symbols = snapshot.symbol_table().expect("Should have symbol table");

    // Extract all identifiers from source
    let identifiers = extract_identifiers(source);

    println!("\n=== USE STATEMENT HIGHLIGHTING ===");
    println!("Total identifiers found: {}", identifiers.len());
    println!("Identifiers: {:?}", identifiers.iter().map(|(n, _, _)| n).collect::<Vec<_>>());
    println!("Total symbol mappings: {}", symbols.span_to_symbol.len());

    // Check each identifier
    let mut missing = Vec::new();
    for (name, start, end) in &identifiers {
        let mut found = false;
        for (span, symbol_id) in &symbols.span_to_symbol {
            if span.start == *start && span.end == *end {
                let info = symbols.symbol_info.get(symbol_id).unwrap();
                println!("✓ '{}' at {}..{} -> {:?} ({})", name, start, end, symbol_id, info.name);
                found = true;
                break;
            }
        }
        if !found {
            println!("✗ '{}' at {}..{} NOT IN SYMBOL TABLE", name, start, end);
            missing.push(name.clone());
        }
    }

    if !missing.is_empty() {
        panic!("Missing symbols: {:?}\nOnly {}/{} identifiers have highlighting",
            missing, identifiers.len() - missing.len(), identifiers.len());
    }
}

#[tokio::test]
async fn test_function_body_highlighting() {
    let source = r#"
fn add(a: num, b: num) -> num {
    let sum = a + b;
    return sum;
}

fn main() -> num {
    let x = 10;
    let y = 20;
    let result = add(x, y);
    return result;
}
"#;

    let (snapshot, _uri, _file_id) = compile_source(source).await;
    let symbols = snapshot.symbol_table().expect("Should have symbol table");

    let identifiers = extract_identifiers(source);

    println!("\n=== FUNCTION BODY HIGHLIGHTING ===");
    println!("Total identifiers found: {}", identifiers.len());
    println!("Total symbol mappings: {}", symbols.span_to_symbol.len());

    // Categorize by what we expect
    let mut params = Vec::new();
    let mut local_vars = Vec::new();
    let mut function_names = Vec::new();
    let mut missing = Vec::new();

    for (name, start, end) in &identifiers {
        let mut found = false;
        for (span, symbol_id) in &symbols.span_to_symbol {
            if span.start == *start && span.end == *end {
                let info = symbols.symbol_info.get(symbol_id).unwrap();
                println!("✓ '{}' at {}..{} -> {:?} ({}, kind={:?})",
                    name, start, end, symbol_id, info.name, info.kind);

                use cantaloop::core::hir_lowering::SymbolKind;
                match info.kind {
                    SymbolKind::Parameter => params.push(name.clone()),
                    SymbolKind::Variable => local_vars.push(name.clone()),
                    SymbolKind::Function => function_names.push(name.clone()),
                    _ => {}
                }
                found = true;
                break;
            }
        }
        if !found {
            println!("✗ '{}' at {}..{} NOT IN SYMBOL TABLE", name, start, end);
            missing.push(name.clone());
        }
    }

    println!("\n=== SUMMARY ===");
    println!("Parameters highlighted: {} {:?}", params.len(), params);
    println!("Local variables highlighted: {} {:?}", local_vars.len(), local_vars);
    println!("Function names highlighted: {} {:?}", function_names.len(), function_names);
    println!("Missing highlighting: {} {:?}", missing.len(), missing);

    if !missing.is_empty() {
        panic!("Missing symbols: {:?}\nOnly {}/{} identifiers have highlighting",
            missing, identifiers.len() - missing.len(), identifiers.len());
    }
}

#[tokio::test]
async fn test_expression_highlighting() {
    let source = r#"
fn calculate(x: num, y: num) -> num {
    let a = x + y;
    let b = a * 2;
    let c = b - x;
    return c / y;
}
"#;

    let (snapshot, _uri, _file_id) = compile_source(source).await;
    let symbols = snapshot.symbol_table().expect("Should have symbol table");

    let identifiers = extract_identifiers(source);

    println!("\n=== EXPRESSION HIGHLIGHTING ===");
    println!("Total identifiers found: {}", identifiers.len());

    let mut missing = Vec::new();
    for (name, start, end) in &identifiers {
        let mut found = false;
        for (span, symbol_id) in &symbols.span_to_symbol {
            if span.start == *start && span.end == *end {
                let info = symbols.symbol_info.get(symbol_id).unwrap();
                println!("✓ '{}' at {}..{} -> {} (kind={:?})",
                    name, start, end, info.name, info.kind);
                found = true;
                break;
            }
        }
        if !found {
            println!("✗ '{}' at {}..{} NOT IN SYMBOL TABLE", name, start, end);
            missing.push(format!("{}@{}", name, start));
        }
    }

    if !missing.is_empty() {
        panic!("Missing symbols: {:?}", missing);
    }
}

#[tokio::test]
async fn test_coverage_percentage() {
    let source = r#"
use print from std;

fn add(a: num, b: num) -> num {
    return a + b;
}

let x = 10;
let y = 20;
let result = add(x, y);
print(result);
"#;

    let (snapshot, _uri, _file_id) = compile_source(source).await;
    let symbols = snapshot.symbol_table().expect("Should have symbol table");

    let identifiers = extract_identifiers(source);

    let mut covered = 0;
    for (_name, start, end) in &identifiers {
        for (span, _) in &symbols.span_to_symbol {
            if span.start == *start && span.end == *end {
                covered += 1;
                break;
            }
        }
    }

    let coverage_pct = (covered as f64 / identifiers.len() as f64) * 100.0;

    println!("\n=== COVERAGE ANALYSIS ===");
    println!("Identifiers in source: {}", identifiers.len());
    println!("Identifiers with symbols: {}", covered);
    println!("Coverage: {:.1}%", coverage_pct);

    // We should have at least 80% coverage
    assert!(coverage_pct >= 80.0,
        "Coverage too low: {:.1}% (expected >= 80%)\nOnly {}/{} identifiers highlighted",
        coverage_pct, covered, identifiers.len());
}

#[tokio::test]
async fn test_use_statement_imports() {
    // This test specifically checks if imported symbols are in the symbol table
    let source = r#"
use print from std;
use map from functional;

print("test");
"#;

    let (snapshot, _uri, _file_id) = compile_source(source).await;
    let symbols = snapshot.symbol_table().expect("Should have symbol table");

    println!("\n=== USE STATEMENT IMPORTS ===");
    println!("All symbols in table:");
    for (symbol_id, info) in &symbols.symbol_info {
        println!("  {:?}: {} (kind={:?})", symbol_id, info.name, info.kind);
    }

    // Check if 'print' and 'map' are in the symbol table
    let has_print = symbols.symbol_info.values().any(|info| info.name == "print");
    let has_map = symbols.symbol_info.values().any(|info| info.name == "map");

    println!("\nhas_print: {}", has_print);
    println!("has_map: {}", has_map);

    // Find the spans for these identifiers in the source
    let print_in_use_start = source.find("print from").unwrap();
    let print_in_call_start = source.find("print(\"test\")").unwrap();
    let map_start = source.find("map from").unwrap();

    println!("\nExpected positions:");
    println!("  'print' in use statement: {}", print_in_use_start);
    println!("  'map' in use statement: {}", map_start);
    println!("  'print' in function call: {}", print_in_call_start);

    println!("\nActual span_to_symbol mappings:");
    for (span, symbol_id) in &symbols.span_to_symbol {
        let info = symbols.symbol_info.get(symbol_id).unwrap();
        println!("  {}..{} -> {} (kind={:?})", span.start, span.end, info.name, info.kind);
    }

    // At minimum, we should have the print call highlighted
    let print_call_span_exists = symbols.span_to_symbol.keys()
        .any(|span| span.start == print_in_call_start && span.end == print_in_call_start + 5);

    assert!(print_call_span_exists,
        "The 'print' function call should be in span_to_symbol map");
}
