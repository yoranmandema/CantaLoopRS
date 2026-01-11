#!/usr/bin/env python3
"""
Batch fix script to add IDs to Spanned::new calls.
Finds Spanned::new(span, node) patterns and converts to Spanned::new(id_gen.next(), span, node)
"""

import re
import sys

def fix_spanned_new_calls(content, functions_with_id_gen):
    """Fix Spanned::new calls to include id_gen.next() where id_gen is available."""
    lines = content.split('\n')
    modified_lines = []
    in_function_with_id_gen = False
    current_function = None
    
    i = 0
    while i < len(lines):
        line = lines[i]
        modified_line = line
        
        # Track which function we're in
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if func_match:
            current_function = func_match.group(3)
            in_function_with_id_gen = current_function in functions_with_id_gen
        
        # Check if we've left the function
        if line.strip() == '}' and i > 0:
            # Check if this closes a function
            brace_count = 0
            for j in range(i, -1, -1):
                if lines[j].strip() == '}':
                    brace_count += 1
                elif lines[j].strip() == '{':
                    brace_count -= 1
                    if brace_count == 0:
                        # Found matching opening brace, might be function end
                        if j > 0 and 'fn ' in lines[j-1]:
                            in_function_with_id_gen = False
                        break
        
        # Fix Spanned::new calls that don't already have id_gen.next()
        if in_function_with_id_gen and 'Spanned::new(' in line and 'id_gen.next()' not in line:
            # Pattern: Spanned::new(span, node)
            # Convert to: Spanned::new(id_gen.next(), span, node)
            pattern = r'Spanned::new\s*\(\s*([^,)]+),\s*([^)]+)\)'
            
            def replace_match(match):
                span_arg = match.group(1).strip()
                node_arg = match.group(2).strip()
                # Skip if id_gen is already in the arguments
                if 'id_gen' in span_arg or 'id_gen' in node_arg:
                    return match.group(0)
                return f'Spanned::new(id_gen.next(), {span_arg}, {node_arg})'
            
            modified_line = re.sub(pattern, replace_match, line)
        
        modified_lines.append(modified_line)
        i += 1
    
    return '\n'.join(modified_lines)

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    functions_file = sys.argv[2] if len(sys.argv) > 2 else None
    
    # Read functions that have id_gen (we'll build this list)
    functions_with_id_gen = set()
    if functions_file:
        with open(functions_file, 'r') as f:
            functions_with_id_gen = set(line.strip() for line in f)
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # First pass: identify functions that have id_gen
    lines = content.split('\n')
    for i, line in enumerate(lines):
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\([^)]*id_gen', line)
        if func_match:
            functions_with_id_gen.add(func_match.group(3))
    
    print(f"Found {len(functions_with_id_gen)} functions with id_gen parameter")
    
    # Apply fixes
    modified_content = fix_spanned_new_calls(content, functions_with_id_gen)
    
    # Write back
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(modified_content)
    
    print(f"Applied fixes to {filepath}")
