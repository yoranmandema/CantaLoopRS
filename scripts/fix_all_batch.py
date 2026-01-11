#!/usr/bin/env python3
"""
Comprehensive batch fix script that applies all fixes in the correct order.
"""

import re
import sys
import subprocess

def get_functions_with_id_gen(content):
    """Get set of functions that have id_gen parameter."""
    functions = set()
    for line in content.split('\n'):
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\([^)]*id_gen', line)
        if func_match:
            functions.add(func_match.group(3))
    return functions

def fix_function_calls(content, functions_with_id_gen):
    """Fix build_cst_* function calls to include id_gen."""
    lines = content.split('\n')
    modified_lines = []
    in_function_with_id_gen = False
    
    for line in lines:
        modified_line = line
        
        # Track function scope
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if func_match:
            in_function_with_id_gen = func_match.group(3) in functions_with_id_gen
        
        # Fix build_cst_expression_from_text
        if in_function_with_id_gen and 'build_cst_expression_from_text(' in line:
            if ', id_gen)' not in line and 'fn build_cst_expression_from_text' not in line:
                pattern = r'build_cst_expression_from_text\s*\(\s*([^,]+),\s*([^,)]+)\s*\)'
                def replace(m):
                    return f'build_cst_expression_from_text({m.group(1).strip()}, {m.group(2).strip()}, id_gen)'
                modified_line = re.sub(pattern, replace, line)
        
        # Fix build_cst_block
        if in_function_with_id_gen and 'build_cst_block(' in line:
            if ', id_gen)' not in line and 'fn build_cst_block' not in line:
                pattern = r'build_cst_block\s*\(\s*([^,)]+)\s*\)'
                def replace(m):
                    return f'build_cst_block({m.group(1).strip()}, id_gen)'
                modified_line = re.sub(pattern, replace, line)
        
        modified_lines.append(modified_line)
    
    return '\n'.join(modified_lines)

def fix_spanned_new_simple(content, functions_with_id_gen):
    """Fix Spanned::new calls - simpler version that just adds id_gen.next() as first arg."""
    lines = content.split('\n')
    modified_lines = []
    in_function_with_id_gen = False
    
    for line in lines:
        modified_line = line
        
        # Track function scope
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if func_match:
            in_function_with_id_gen = func_match.group(3) in functions_with_id_gen
        
        # Fix Spanned::new calls
        if in_function_with_id_gen and 'Spanned::new(' in line:
            if 'id_gen.next()' not in line:
                # Simple pattern: Spanned::new(span, node) -> Spanned::new(id_gen.next(), span, node)
                # But be careful with nested calls
                pattern = r'Spanned::new\s*\(\s*([^,)]+),\s*([^)]+)\)'
                
                def replace(m):
                    span_arg = m.group(1).strip()
                    node_arg = m.group(2).strip()
                    # Skip if id_gen is already in args
                    if 'id_gen' in span_arg or 'id_gen' in node_arg:
                        return m.group(0)
                    return f'Spanned::new(id_gen.next(), {span_arg}, {node_arg})'
                
                modified_line = re.sub(pattern, replace, line)
        
        modified_lines.append(modified_line)
    
    return '\n'.join(modified_lines)

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    dry_run = '--dry-run' in sys.argv
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Get functions that already have id_gen
    functions_with_id_gen = get_functions_with_id_gen(content)
    print(f"Found {len(functions_with_id_gen)} functions with id_gen parameter")
    
    if not dry_run:
        # Apply fixes in order
        print("Fixing function calls...")
        content = fix_function_calls(content, functions_with_id_gen)
        
        # Update functions_with_id_gen after potential fixes
        functions_with_id_gen = get_functions_with_id_gen(content)
        
        print("Fixing Spanned::new calls...")
        content = fix_spanned_new_simple(content, functions_with_id_gen)
        
        # Write back
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
        
        print(f"Applied batch fixes to {filepath}")
    else:
        print("Dry run - no changes made")
