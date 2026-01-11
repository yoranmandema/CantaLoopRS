#!/usr/bin/env python3
"""
Script to help update Spanned::new calls to include IDs.
This is a helper script - changes should be reviewed carefully.
"""

import re
import sys

def find_spanned_new_calls(content):
    """Find all Spanned::new calls that need IDs."""
    lines = content.split('\n')
    matches = []
    
    # Pattern: Spanned::new(span, node) - two arguments
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
                
                # Skip if id_gen is already in the arguments
                if 'id_gen' in span_arg or 'id_gen' in node_arg:
                    continue
                
                matches.append({
                    'line_num': i,
                    'line': line,
                    'span_arg': span_arg,
                    'node_arg': node_arg,
                    'full_match': match.group(0),
                })
    
    return matches

def analyze_function_scope(content, line_num):
    """Analyze if a function has id_gen in scope at a given line."""
    lines = content.split('\n')
    
    # Find the function containing this line
    for i in range(line_num - 1, -1, -1):
        line = lines[i]
        # Check function signature
        if re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+\w+\s*\([^)]*id_gen.*\)', line):
            return True
        # Check if we've left the function
        if i < line_num - 1 and line.strip() == '}':
            break
    
    return False

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: python fix_spanned_new.py <file>")
        sys.exit(1)
    
    filepath = sys.argv[1]
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    matches = find_spanned_new_calls(content)
    
    print(f"Found {len(matches)} Spanned::new calls that may need IDs")
    print(f"\nFirst 20:")
    for match in matches[:20]:
        in_scope = analyze_function_scope(content, match['line_num'])
        scope_str = "HAS id_gen" if in_scope else "NO id_gen"
        print(f"Line {match['line_num']} ({scope_str}):")
        print(f"  {match['line'].strip()[:80]}")
        print(f"  -> Spanned::new(id_gen.next(), {match['span_arg'][:30]}..., {match['node_arg'][:30]}...)")
        print()
