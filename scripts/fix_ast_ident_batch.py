#!/usr/bin/env python3
"""Fix AST identifier issues: String -> AstIdent conversions"""

import re
import sys

def fix_ast_builder_identifier(file_path):
    """Fix build_identifier_expr to return AstIdent"""
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Fix build_identifier_expr to create AstIdent
    old_pattern = r'fn build_identifier_expr\(pair: Pair<Rule>\) -> Result<Expression, pest::error::Error<Rule>> \{\s+let identifier = pair\.as_str\(\)\.to_string\(\);'
    new_code = '''fn build_identifier_expr(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let identifier_str = pair.as_str().to_string();
    let identifier = crate::core::ast::AstIdent {
        name: identifier_str.clone(),
        cst_id: crate::core::cst::CstId::new(0), // TODO: Get actual CstId from CST
    };'''
    
    content = re.sub(old_pattern, new_code, content)
    
    # Fix the keyword check to use identifier.name
    content = re.sub(
        r'if KEYWORDS\.contains\(&identifier\.as_str\(\)\)',
        'if KEYWORDS.contains(&identifier.name.as_str())',
        content
    )
    
    # Fix the error message to use identifier.name
    content = re.sub(
        r"message: format!\('{}' is a keyword",
        "message: format!('{}' is a keyword",
        content
    )
    content = re.sub(
        r"format!\('{}' is a keyword and cannot be used as an identifier\", identifier\)",
        "format!('{}' is a keyword and cannot be used as an identifier', identifier.name)",
        content
    )
    
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"Fixed {file_path}")

def fix_struct_init_fields(file_path):
    """Fix struct init fields to use AstIdent"""
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Fix field_name to be AstIdent
    old_pattern = r'let field_name = identifier_text\.to_string\(\);'
    new_code = '''let field_name = crate::core::ast::AstIdent {
        name: identifier_text.to_string(),
        cst_id: crate::core::cst::CstId::new(0), // TODO: Get actual CstId from CST
    };'''
    
    content = re.sub(old_pattern, new_code, content)
    
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"Fixed struct init fields in {file_path}")

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: fix_ast_ident_batch.py <file>")
        sys.exit(1)
    
    file_path = sys.argv[1]
    if 'builder.rs' in file_path:
        fix_ast_builder_identifier(file_path)
        fix_struct_init_fields(file_path)
    else:
        print(f"Unknown file: {file_path}")
