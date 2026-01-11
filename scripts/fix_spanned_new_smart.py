#!/usr/bin/env python3
"""
Smart script to fix Spanned::new calls - avoids duplicates.
Only adds id_gen.next() if the pattern is Spanned::new(span, node) (2 args).
Skips if already has id_gen.next() or if it's Spanned::new(id, ...) pattern.
"""

import re
import sys

def fix_spanned_new_smart(content, functions_with_id_gen):
    """Fix Spanned::new calls smartly, avoiding duplicates."""
    lines = content.split('\n')
    modified_lines = []
    in_function_with_id_gen = False
    
    for line in lines:
        modified_line = line
        
        # Track function scope
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if func_match:
            in_function_with_id_gen = func_match.group(3) in functions_with_id_gen
        
        # Fix Spanned::new calls - but be smart about it
        if in_function_with_id_gen and 'Spanned::new(' in line:
            # Skip if already has id_gen.next()
            if 'id_gen.next()' in line:
                modified_lines.append(modified_line)
                continue
            
            # Pattern: Spanned::new(span, node) - exactly 2 arguments, no id
            # But NOT: Spanned::new(id, span, node) - already has id
            pattern = r'Spanned::new\s*\(\s*([^,)]+),\s*([^)]+)\)'
            
            def replace_match(match):
                first_arg = match.group(1).strip()
                second_arg = match.group(2).strip()
                
                # Skip if id_gen is already in the arguments
                if 'id_gen' in first_arg or 'id_gen' in second_arg:
                    return match.group(0)
                
                # Skip if first arg looks like an id variable (id, ident_id, etc.)
                # These are patterns like: Spanned::new(id, span, ...)
                if re.match(r'^(id|ident_id|lit_id|obj_id|obj_ident_id)\s*,\s*', match.group(0)):
                    return match.group(0)
                
                # Only fix if it's the simple pattern: Spanned::new(span, node)
                return f'Spanned::new(id_gen.next(), {first_arg}, {second_arg})'
            
            # Only replace if it matches the 2-arg pattern and doesn't already have id
            if re.search(r'Spanned::new\s*\(\s*[^,)]+,\s*[^)]+\)', line):
                # Check if it's NOT already Spanned::new(id, ...) pattern
                if not re.search(r'Spanned::new\s*\(\s*(id|ident_id|lit_id|obj_id)\s*,', line):
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
    
    modified_content = fix_spanned_new_smart(content, functions_with_id_gen)
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(modified_content)
    
    print(f"Applied smart fixes to {filepath}")
