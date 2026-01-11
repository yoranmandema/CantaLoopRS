#!/usr/bin/env python3
"""
Analyze builder.rs to identify functions that need id_gen parameters
and Spanned::new calls that need IDs.
"""

import re
import sys

def analyze_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
        lines = content.split('\n')
    
    # Find all function definitions
    functions = []
    current_func = None
    func_start = None
    func_lines = []
    
    for i, line in enumerate(lines, 1):
        # Match function definition
        match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if match:
            # Save previous function
            if current_func:
                functions.append({
                    'name': current_func,
                    'start': func_start,
                    'end': i - 1,
                    'lines': func_lines,
                    'signature': func_lines[0] if func_lines else '',
                })
            
            # Start new function
            current_func = match.group(3)
            func_start = i
            func_lines = [line]
        elif current_func:
            func_lines.append(line)
            # Check for function end (closing brace at start of line)
            if line.strip() == '}' and len([l for l in func_lines if '{' in l]) > 0:
                brace_count = sum(l.count('{') for l in func_lines) - sum(l.count('}') for l in func_lines)
                if brace_count == 0:
                    functions.append({
                        'name': current_func,
                        'start': func_start,
                        'end': i,
                        'lines': func_lines,
                        'signature': func_lines[0] if func_lines else '',
                    })
                    current_func = None
                    func_lines = []
    
    # Analyze functions
    functions_needing_id_gen = []
    for func in functions:
        sig = func['signature']
        body = '\n'.join(func['lines'])
        
        has_id_gen_param = 'id_gen: &mut CstIdGenerator' in sig
        has_spanned_new = 'Spanned::new(' in body
        calls_build_cst = 'build_cst_' in body
        
        if (has_spanned_new or calls_build_cst) and not has_id_gen_param:
            functions_needing_id_gen.append(func)
    
    return functions_needing_id_gen

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    
    functions = analyze_file(filepath)
    
    print(f"Found {len(functions)} functions that need id_gen parameter:")
    print("\nCritical path functions (build_cst_*):")
    critical = [f for f in functions if f['name'].startswith('build_cst_')]
    for func in critical[:20]:
        print(f"  {func['name']} (line {func['start']})")
        # Count Spanned::new calls
        spanned_count = func['lines'].count('Spanned::new(') - func['lines'].count('id_gen.next()')
        print(f"    - Spanned::new calls needing IDs: ~{spanned_count}")
