#!/usr/bin/env python3
"""
Fix remaining Spanned::new calls that need IDs.
This script focuses on specific patterns that need fixing.
"""

import re
import sys

def fix_remaining_spanned_new(content, functions_with_id_gen):
    """Fix remaining Spanned::new calls."""
    lines = content.split('\n')
    modified_lines = []
    in_function_with_id_gen = False
    current_function = None
    
    for i, line in enumerate(lines):
        modified_line = line
        
        # Track function scope
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if func_match:
            current_function = func_match.group(3)
            in_function_with_id_gen = current_function in functions_with_id_gen
        
        # Fix Spanned::new calls that need IDs
        if in_function_with_id_gen and 'Spanned::new(' in line:
            # Skip if already has id_gen.next()
            if 'id_gen.next()' in line:
                modified_lines.append(modified_line)
                continue
            
            # Skip if it's Spanned::new(id, ...) pattern (already has ID)
            if re.search(r'Spanned::new\s*\(\s*(id|ident_id|lit_id|obj_id|obj_ident_id)\s*,', line):
                modified_lines.append(modified_line)
                continue
            
            # Pattern: Spanned::new(span, node) - 2 arguments
            # Convert to: Spanned::new(id_gen.next(), span, node)
            pattern = r'Spanned::new\s*\(\s*([^,)]+),\s*([^)]+)\)'
            
            def replace_match(match):
                first_arg = match.group(1).strip()
                second_arg = match.group(2).strip()
                
                # Skip if id_gen is already in the arguments
                if 'id_gen' in first_arg or 'id_gen' in second_arg:
                    return match.group(0)
                
                # Add id_gen.next() as first argument
                return f'Spanned::new(id_gen.next(), {first_arg}, {second_arg})'
            
            modified_line = re.sub(pattern, replace_match, line)
        
        modified_lines.append(modified_line)
    
    return '\n'.join(modified_lines)

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Get functions with id_gen
    functions_with_id_gen = set()
    for line in content.split('\n'):
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\([^)]*id_gen', line)
        if func_match:
            functions_with_id_gen.add(func_match.group(3))
    
    print(f"Found {len(functions_with_id_gen)} functions with id_gen parameter")
    
    modified_content = fix_remaining_spanned_new(content, functions_with_id_gen)
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(modified_content)
    
    print(f"Applied fixes to {filepath}")
