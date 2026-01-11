#!/usr/bin/env python3
"""
Fix function calls in build_cst_statement to pass id_gen.
"""

import re
import sys

def fix_statement_calls(content):
    """Fix calls to statement builder functions in build_cst_statement."""
    lines = content.split('\n')
    modified_lines = []
    in_build_cst_statement = False
    
    # Functions that need id_gen parameter
    functions_needing_id_gen = [
        'build_cst_let_statement', 'build_cst_const_statement', 'build_cst_assign_statement',
        'build_cst_assign_increment_statement', 'build_cst_assign_decrement_statement',
        'build_cst_if_statement', 'build_cst_match_statement', 'build_cst_function_declaration',
        'build_cst_return_statement', 'build_cst_loop_statement', 'build_cst_while_statement',
        'build_cst_for_statement', 'build_cst_break_statement', 'build_cst_use_statement',
        'build_cst_struct_statement',
    ]
    
    for line in lines:
        modified_line = line
        
        # Track if we're in build_cst_statement
        if re.match(r'^\s*fn build_cst_statement\s*\(', line):
            in_build_cst_statement = True
        elif in_build_cst_statement and line.strip() == '}':
            in_build_cst_statement = False
        
        # Fix calls within build_cst_statement
        if in_build_cst_statement:
            for func_name in functions_needing_id_gen:
                pattern = r'(\s+)(' + re.escape(func_name) + r')\s*\(\s*([^,)]+)\s*\)'
                if re.search(pattern, line) and ', id_gen)' not in line:
                    def replace(m):
                        indent = m.group(1)
                        func = m.group(2)
                        arg = m.group(3).strip()
                        return f'{indent}{func}({arg}, id_gen)'
                    modified_line = re.sub(pattern, replace, line)
                    break
        
        modified_lines.append(modified_line)
    
    return '\n'.join(modified_lines)

if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'src/core/cst/builder.rs'
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    modified_content = fix_statement_calls(content)
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(modified_content)
    
    print(f"Fixed statement builder calls in build_cst_statement")
