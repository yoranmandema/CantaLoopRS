#!/usr/bin/env python3
"""
Batch fix script to add id_gen parameter to build_cst_expression_from_text and build_cst_block calls.
"""

import re
import sys

def fix_function_calls(content, functions_needing_id_gen):
    """Fix function calls to include id_gen parameter."""
    lines = content.split('\n')
    modified_lines = []
    in_function_with_id_gen = False
    current_function = None
    
    for i, line in enumerate(lines):
        modified_line = line
        
        # Track which function we're in
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if func_match:
            current_function = func_match.group(3)
            in_function_with_id_gen = current_function in functions_needing_id_gen
        
        # Fix build_cst_expression_from_text calls
        if in_function_with_id_gen and 'build_cst_expression_from_text(' in line and ', id_gen)' not in line:
            # Pattern: build_cst_expression_from_text(text, span)
            # Convert to: build_cst_expression_from_text(text, span, id_gen)
            pattern = r'build_cst_expression_from_text\s*\(\s*([^,]+),\s*([^,)]+)\s*\)'
            def replace_match(match):
                text_arg = match.group(1).strip()
                span_arg = match.group(2).strip()
                return f'build_cst_expression_from_text({text_arg}, {span_arg}, id_gen)'
            modified_line = re.sub(pattern, replace_match, line)
        
        # Fix build_cst_block calls
        if in_function_with_id_gen and 'build_cst_block(' in line and ', id_gen)' not in line:
            # Pattern: build_cst_block(pair)
            # Convert to: build_cst_block(pair, id_gen)
            pattern = r'build_cst_block\s*\(\s*([^,)]+)\s*\)'
            def replace_match(match):
                arg = match.group(1).strip()
                return f'build_cst_block({arg}, id_gen)'
            modified_line = re.sub(pattern, replace_match, line)
        
        modified_lines.append(modified_line)
    
    return '\n'.join(modified_lines)

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Identify functions that have id_gen parameter
    lines = content.split('\n')
    functions_with_id_gen = set()
    for i, line in enumerate(lines):
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\([^)]*id_gen', line)
        if func_match:
            functions_with_id_gen.add(func_match.group(3))
    
    print(f"Found {len(functions_with_id_gen)} functions with id_gen parameter")
    
    # Apply fixes
    modified_content = fix_function_calls(content, functions_with_id_gen)
    
    # Write back
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(modified_content)
    
    print(f"Applied fixes to {filepath}")
