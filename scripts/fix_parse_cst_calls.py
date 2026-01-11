#!/usr/bin/env python3
"""
Fix calls to parse_cst_* functions to pass id_gen.
"""

import re
import sys

def fix_parse_cst_calls(content, functions_with_id_gen):
    """Fix parse_cst_* function calls to include id_gen."""
    lines = content.split('\n')
    modified_lines = []
    in_function_with_id_gen = False
    
    funcs_to_fix = ['parse_cst_function_arguments', 'parse_cst_expression_list', 
                    'parse_cst_call_argument', 'parse_cst_index_spec']
    
    for line in lines:
        modified_line = line
        
        # Track function scope
        func_match = re.match(r'^\s*(pub\s+)?(pub\(crate\)\s+)?fn\s+(\w+)\s*\(', line)
        if func_match:
            in_function_with_id_gen = func_match.group(3) in functions_with_id_gen
        
        # Fix parse_cst_function_arguments
        if in_function_with_id_gen and 'parse_cst_function_arguments(' in line and ', id_gen)' not in line:
            if 'fn parse_cst_function_arguments' not in line:
                pattern = r'parse_cst_function_arguments\s*\(\s*([^,]+),\s*([^,)]+)\s*\)'
                def replace(m):
                    return f'parse_cst_function_arguments({m.group(1).strip()}, {m.group(2).strip()}, id_gen)'
                modified_line = re.sub(pattern, replace, line)
        
        # Fix parse_cst_expression_list
        if in_function_with_id_gen and 'parse_cst_expression_list(' in line and ', id_gen)' not in line:
            if 'fn parse_cst_expression_list' not in line:
                pattern = r'parse_cst_expression_list\s*\(\s*([^,]+),\s*([^,)]+)\s*\)'
                def replace(m):
                    return f'parse_cst_expression_list({m.group(1).strip()}, {m.group(2).strip()}, id_gen)'
                modified_line = re.sub(pattern, replace, line)
        
        # Fix parse_cst_call_argument
        if in_function_with_id_gen and 'parse_cst_call_argument(' in line and ', id_gen)' not in line:
            if 'fn parse_cst_call_argument' not in line:
                pattern = r'parse_cst_call_argument\s*\(\s*([^,]+),\s*([^,)]+)\s*\)'
                def replace(m):
                    return f'parse_cst_call_argument({m.group(1).strip()}, {m.group(2).strip()}, id_gen)'
                modified_line = re.sub(pattern, replace, line)
        
        # Fix parse_cst_index_spec
        if in_function_with_id_gen and 'parse_cst_index_spec(' in line and ', id_gen)' not in line:
            if 'fn parse_cst_index_spec' not in line:
                pattern = r'parse_cst_index_spec\s*\(\s*([^,]+),\s*([^,)]+)\s*\)'
                def replace(m):
                    return f'parse_cst_index_spec({m.group(1).strip()}, {m.group(2).strip()}, id_gen)'
                modified_line = re.sub(pattern, replace, line)
        
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
    
    modified_content = fix_parse_cst_calls(content, functions_with_id_gen)
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(modified_content)
    
    print(f"Fixed parse_cst_* function calls")
