#!/usr/bin/env python3
"""
Batch script to add id_gen parameters to function signatures that need them.
"""

import re
import sys

def add_id_gen_to_signature(signature_line):
    """Add id_gen parameter to function signature."""
    param_start = signature_line.find('(')
    param_end = signature_line.rfind(')')
    if param_start == -1 or param_end == -1:
        return signature_line
    
    params = signature_line[param_start+1:param_end].strip()
    if 'id_gen' in params:
        return signature_line  # Already has id_gen
    
    if params:
        new_params = params + ', id_gen: &mut CstIdGenerator'
    else:
        new_params = 'id_gen: &mut CstIdGenerator'
    
    return signature_line[:param_start+1] + new_params + signature_line[param_end:]

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    functions_to_fix = sys.argv[2:] if len(sys.argv) > 2 else []
    
    with open(filepath, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    
    # Functions that need id_gen (from analysis)
    if not functions_to_fix:
        functions_to_fix = [
            'build_cst_let_statement', 'build_cst_const_statement', 'build_cst_assign_statement',
            'build_cst_assign_increment_statement', 'build_cst_assign_decrement_statement',
            'build_cst_if_statement', 'build_cst_match_statement', 'build_cst_function_declaration',
            'build_cst_return_statement', 'build_cst_loop_statement', 'build_cst_while_statement',
            'build_cst_for_statement', 'build_cst_break_statement', 'build_cst_use_statement',
            'build_cst_struct_statement', 'parse_cst_function_arguments', 'parse_cst_call_argument',
            'parse_cst_expression_list', 'parse_cst_index_spec',
        ]
    
    modified = False
    for i, line in enumerate(lines):
        for func_name in functions_to_fix:
            pattern = r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+' + re.escape(func_name) + r'\s*\('
            if re.match(pattern, line) and 'id_gen' not in line:
                lines[i] = add_id_gen_to_signature(line)
                modified = True
                print(f"Fixed {func_name} at line {i+1}")
                break
    
    if modified:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.writelines(lines)
        print(f"Applied fixes to {filepath}")
    else:
        print("No changes needed")
