#!/usr/bin/env python3
"""
Batch fix script to help identify and fix Spanned::new calls and function signatures.
This is a helper - manual review is still needed.
"""

import re
import sys

def find_all_spanned_new_calls(content):
    """Find all Spanned::new calls that need IDs."""
    lines = content.split('\n')
    issues = []
    
    # Pattern: Spanned::new(span, node) - two arguments (missing id)
    # But not: Spanned::new(id_gen.next(), ...) - already has id
    pattern = r'Spanned::new\s*\(\s*([^,]+),\s*([^)]+)\)'
    
    for i, line in enumerate(lines, 1):
        if 'Spanned::new(' in line:
            # Skip if already has id_gen.next()
            if 'id_gen.next()' in line:
                continue
            
            # Find matches
            for match in re.finditer(pattern, line):
                span_arg = match.group(1).strip()
                node_arg = match.group(2).strip()
                
                # Skip if id_gen is already in the arguments (might be nested)
                if 'id_gen' in span_arg or 'id_gen' in node_arg:
                    continue
                
                issues.append({
                    'line_num': i,
                    'line': line,
                    'span_arg': span_arg,
                    'node_arg': node_arg,
                })
    
    return issues

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    issues = find_all_spanned_new_calls(content)
    
    print(f"Found {len(issues)} Spanned::new calls that may need IDs")
    print(f"\nFirst 30:")
    for issue in issues[:30]:
        print(f"Line {issue['line_num']}:")
        print(f"  {issue['line'].strip()[:100]}")
