#!/usr/bin/env python3
"""
Comprehensive script to analyze functions and suggest/add id_gen parameters.
"""

import re
import sys

def analyze_functions(content):
    """Analyze which functions need id_gen parameters."""
    lines = content.split('\n')
    functions = {}
    current_func = None
    func_start = None
    func_lines = []
    brace_count = 0
    
    for i, line in enumerate(lines):
        # Detect function start
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if func_match:
            # Save previous function
            if current_func:
                functions[current_func] = {
                    'start': func_start,
                    'end': i - 1,
                    'has_id_gen': 'id_gen: &mut CstIdGenerator' in func_lines[0] if func_lines else False,
                    'uses_build_cst': any('build_cst_' in l for l in func_lines),
                    'uses_spanned_new': any('Spanned::new(' in l and 'id_gen.next()' not in l for l in func_lines),
                }
            
            # Start new function
            current_func = func_match.group(3)
            func_start = i
            func_lines = [line]
            brace_count = line.count('{') - line.count('}')
        elif current_func:
            func_lines.append(line)
            brace_count += line.count('{') - line.count('}')
            if brace_count == 0 and func_lines:
                # Function ended
                functions[current_func] = {
                    'start': func_start,
                    'end': i,
                    'has_id_gen': 'id_gen: &mut CstIdGenerator' in func_lines[0] if func_lines else False,
                    'uses_build_cst': any('build_cst_' in l for l in func_lines),
                    'uses_spanned_new': any('Spanned::new(' in l and 'id_gen.next()' not in l for l in func_lines),
                }
                current_func = None
                func_lines = []
    
    return functions

def add_id_gen_to_signature(signature_line):
    """Add id_gen parameter to function signature."""
    # Find parameter list
    param_start = signature_line.find('(')
    param_end = signature_line.rfind(')')
    if param_start == -1 or param_end == -1:
        return signature_line
    
    params = signature_line[param_start+1:param_end].strip()
    if params:
        new_params = params + ', id_gen: &mut CstIdGenerator'
    else:
        new_params = 'id_gen: &mut CstIdGenerator'
    
    return signature_line[:param_start+1] + new_params + signature_line[param_end:]

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    dry_run = '--dry-run' in sys.argv
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    functions = analyze_functions(content)
    lines = content.split('\n')
    
    # Find functions that need id_gen
    functions_needing_id_gen = []
    for func_name, info in functions.items():
        if not info['has_id_gen'] and (info['uses_build_cst'] or info['uses_spanned_new']):
            functions_needing_id_gen.append((func_name, info))
    
    print(f"Found {len(functions_needing_id_gen)} functions that need id_gen parameter:")
    for func_name, info in sorted(functions_needing_id_gen, key=lambda x: x[1]['start'])[:30]:
        print(f"  {func_name} (line {info['start']+1}): uses_build_cst={info['uses_build_cst']}, uses_spanned_new={info['uses_spanned_new']}")
    
    if not dry_run and functions_needing_id_gen:
        # Apply fixes to function signatures
        modified_lines = lines[:]
        for func_name, info in functions_needing_id_gen:
            sig_line_idx = info['start']
            if sig_line_idx < len(modified_lines):
                modified_lines[sig_line_idx] = add_id_gen_to_signature(modified_lines[sig_line_idx])
        
        # Write back
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write('\n'.join(modified_lines))
        
        print(f"\nApplied fixes to {len(functions_needing_id_gen)} function signatures")
    else:
        print("\nDry run - no changes made. Use without --dry-run to apply fixes.")
