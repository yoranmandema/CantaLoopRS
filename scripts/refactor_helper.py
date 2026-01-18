#!/usr/bin/env python3
"""
CantaLoopRS Refactoring Helper

Uses tree-sitter-rust to parse and analyze Rust code for refactoring tasks.
This provides more accurate results than regex-based search.

Installation:
    pip install tree-sitter tree-sitter-rust

Usage:
    python scripts/refactor_helper.py find-struct StructName
    python scripts/refactor_helper.py find-impl StructName
    python scripts/refactor_helper.py find-calls function_name
    python scripts/refactor_helper.py find-field struct_name field_name
    python scripts/refactor_helper.py find-enum EnumName
    python scripts/refactor_helper.py find-trait TraitName
    python scripts/refactor_helper.py list-types [directory]
"""

import sys
import os
from pathlib import Path
from typing import List, Dict, Tuple, Optional
import json

try:
    from tree_sitter import Language, Parser, Node
    import tree_sitter_rust as tsrust
except ImportError:
    print("Error: Required packages not installed.", file=sys.stderr)
    print("Install with: pip install tree-sitter tree-sitter-rust", file=sys.stderr)
    sys.exit(1)


class RustAnalyzer:
    def __init__(self, root_dir: str = "."):
        self.root_dir = Path(root_dir)
        self.parser = Parser()
        self.rust_language = Language(tsrust.language())
        self.parser.set_language(self.rust_language)

    def find_rust_files(self, directory: Optional[str] = None) -> List[Path]:
        """Find all Rust files in the project."""
        search_dir = Path(directory) if directory else self.root_dir

        # Skip target directory
        rust_files = []
        for path in search_dir.rglob("*.rs"):
            if "target" not in path.parts:
                rust_files.append(path)

        return rust_files

    def parse_file(self, file_path: Path) -> Optional[Node]:
        """Parse a Rust file and return the syntax tree."""
        try:
            with open(file_path, 'rb') as f:
                source_code = f.read()
            tree = self.parser.parse(source_code)
            return tree.root_node
        except Exception as e:
            print(f"Error parsing {file_path}: {e}", file=sys.stderr)
            return None

    def get_node_text(self, node: Node, source: bytes) -> str:
        """Extract text content from a node."""
        return source[node.start_byte:node.end_byte].decode('utf-8')

    def find_structs(self, name: Optional[str] = None) -> List[Dict]:
        """Find all struct definitions, optionally filtered by name."""
        results = []

        for file_path in self.find_rust_files():
            with open(file_path, 'rb') as f:
                source = f.read()

            root = self.parse_file(file_path)
            if not root:
                continue

            # Query for struct definitions
            query = self.rust_language.query("""
                (struct_item
                    name: (type_identifier) @struct_name
                ) @struct
            """)

            captures = query.captures(root)
            for node, capture_name in captures:
                if capture_name == "struct":
                    struct_name_node = node.child_by_field_name("name")
                    if struct_name_node:
                        struct_name = self.get_node_text(struct_name_node, source)

                        if name is None or struct_name == name:
                            results.append({
                                "file": str(file_path.relative_to(self.root_dir)),
                                "name": struct_name,
                                "line": node.start_point[0] + 1,
                                "column": node.start_point[1] + 1,
                                "end_line": node.end_point[0] + 1,
                                "type": "struct"
                            })

        return results

    def find_impls(self, type_name: str) -> List[Dict]:
        """Find all impl blocks for a given type."""
        results = []

        for file_path in self.find_rust_files():
            with open(file_path, 'rb') as f:
                source = f.read()

            root = self.parse_file(file_path)
            if not root:
                continue

            # Query for impl blocks
            query = self.rust_language.query("""
                (impl_item
                    type: (type_identifier) @impl_type
                ) @impl
            """)

            captures = query.captures(root)
            for node, capture_name in captures:
                if capture_name == "impl":
                    type_node = node.child_by_field_name("type")
                    if type_node:
                        impl_type = self.get_node_text(type_node, source)

                        if impl_type == type_name:
                            results.append({
                                "file": str(file_path.relative_to(self.root_dir)),
                                "type": impl_type,
                                "line": node.start_point[0] + 1,
                                "column": node.start_point[1] + 1,
                                "end_line": node.end_point[0] + 1,
                            })

        return results

    def find_function_calls(self, function_name: str) -> List[Dict]:
        """Find all calls to a specific function."""
        results = []

        for file_path in self.find_rust_files():
            with open(file_path, 'rb') as f:
                source = f.read()

            root = self.parse_file(file_path)
            if not root:
                continue

            # Query for function calls
            query = self.rust_language.query("""
                (call_expression
                    function: (identifier) @func_name
                ) @call
            """)

            captures = query.captures(root)
            for node, capture_name in captures:
                if capture_name == "call":
                    func_node = node.child_by_field_name("function")
                    if func_node:
                        func_name = self.get_node_text(func_node, source)

                        if func_name == function_name:
                            results.append({
                                "file": str(file_path.relative_to(self.root_dir)),
                                "function": func_name,
                                "line": node.start_point[0] + 1,
                                "column": node.start_point[1] + 1,
                                "context": self.get_node_text(node, source)[:100]
                            })

        return results

    def find_struct_fields(self, struct_name: str, field_name: Optional[str] = None) -> List[Dict]:
        """Find struct field definitions."""
        results = []

        for file_path in self.find_rust_files():
            with open(file_path, 'rb') as f:
                source = f.read()

            root = self.parse_file(file_path)
            if not root:
                continue

            query = self.rust_language.query("""
                (struct_item
                    name: (type_identifier) @struct_name
                    body: (field_declaration_list) @fields
                ) @struct
            """)

            captures = query.captures(root)
            for node, capture_name in captures:
                if capture_name == "struct":
                    name_node = node.child_by_field_name("name")
                    if name_node and self.get_node_text(name_node, source) == struct_name:
                        # Find fields
                        body = node.child_by_field_name("body")
                        if body:
                            for child in body.named_children:
                                if child.type == "field_declaration":
                                    field_name_node = child.child_by_field_name("name")
                                    if field_name_node:
                                        fn = self.get_node_text(field_name_node, source)
                                        if field_name is None or fn == field_name:
                                            results.append({
                                                "file": str(file_path.relative_to(self.root_dir)),
                                                "struct": struct_name,
                                                "field": fn,
                                                "line": child.start_point[0] + 1,
                                                "column": child.start_point[1] + 1
                                            })

        return results

    def find_enums(self, name: Optional[str] = None) -> List[Dict]:
        """Find all enum definitions."""
        results = []

        for file_path in self.find_rust_files():
            with open(file_path, 'rb') as f:
                source = f.read()

            root = self.parse_file(file_path)
            if not root:
                continue

            query = self.rust_language.query("""
                (enum_item
                    name: (type_identifier) @enum_name
                ) @enum
            """)

            captures = query.captures(root)
            for node, capture_name in captures:
                if capture_name == "enum":
                    name_node = node.child_by_field_name("name")
                    if name_node:
                        enum_name = self.get_node_text(name_node, source)

                        if name is None or enum_name == name:
                            results.append({
                                "file": str(file_path.relative_to(self.root_dir)),
                                "name": enum_name,
                                "line": node.start_point[0] + 1,
                                "column": node.start_point[1] + 1,
                                "end_line": node.end_point[0] + 1,
                                "type": "enum"
                            })

        return results

    def find_traits(self, name: Optional[str] = None) -> List[Dict]:
        """Find all trait definitions."""
        results = []

        for file_path in self.find_rust_files():
            with open(file_path, 'rb') as f:
                source = f.read()

            root = self.parse_file(file_path)
            if not root:
                continue

            query = self.rust_language.query("""
                (trait_item
                    name: (type_identifier) @trait_name
                ) @trait
            """)

            captures = query.captures(root)
            for node, capture_name in captures:
                if capture_name == "trait":
                    name_node = node.child_by_field_name("name")
                    if name_node:
                        trait_name = self.get_node_text(name_node, source)

                        if name is None or trait_name == name:
                            results.append({
                                "file": str(file_path.relative_to(self.root_dir)),
                                "name": trait_name,
                                "line": node.start_point[0] + 1,
                                "column": node.start_point[1] + 1,
                                "end_line": node.end_point[0] + 1,
                                "type": "trait"
                            })

        return results

    def list_all_types(self, directory: Optional[str] = None) -> Dict[str, List[str]]:
        """List all types (structs, enums, traits) in the codebase."""
        structs = [s["name"] for s in self.find_structs()]
        enums = [e["name"] for e in self.find_enums()]
        traits = [t["name"] for t in self.find_traits()]

        return {
            "structs": sorted(set(structs)),
            "enums": sorted(set(enums)),
            "traits": sorted(set(traits))
        }


def print_results(results: List[Dict], format: str = "human"):
    """Print results in the specified format."""
    if format == "json":
        print(json.dumps(results, indent=2))
    else:
        for result in results:
            file = result.get("file", "")
            line = result.get("line", 0)
            print(f"{file}:{line}")
            for key, value in result.items():
                if key not in ["file", "line"]:
                    print(f"  {key}: {value}")
            print()


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    command = sys.argv[1]
    analyzer = RustAnalyzer()

    try:
        if command == "find-struct":
            if len(sys.argv) < 3:
                results = analyzer.find_structs()
            else:
                name = sys.argv[2]
                results = analyzer.find_structs(name)
            print_results(results)

        elif command == "find-impl":
            if len(sys.argv) < 3:
                print("Usage: refactor_helper.py find-impl <type_name>")
                sys.exit(1)
            type_name = sys.argv[2]
            results = analyzer.find_impls(type_name)
            print_results(results)

        elif command == "find-calls":
            if len(sys.argv) < 3:
                print("Usage: refactor_helper.py find-calls <function_name>")
                sys.exit(1)
            function_name = sys.argv[2]
            results = analyzer.find_function_calls(function_name)
            print_results(results)

        elif command == "find-field":
            if len(sys.argv) < 3:
                print("Usage: refactor_helper.py find-field <struct_name> [field_name]")
                sys.exit(1)
            struct_name = sys.argv[2]
            field_name = sys.argv[3] if len(sys.argv) > 3 else None
            results = analyzer.find_struct_fields(struct_name, field_name)
            print_results(results)

        elif command == "find-enum":
            if len(sys.argv) < 3:
                results = analyzer.find_enums()
            else:
                name = sys.argv[2]
                results = analyzer.find_enums(name)
            print_results(results)

        elif command == "find-trait":
            if len(sys.argv) < 3:
                results = analyzer.find_traits()
            else:
                name = sys.argv[2]
                results = analyzer.find_traits(name)
            print_results(results)

        elif command == "list-types":
            directory = sys.argv[2] if len(sys.argv) > 2 else None
            types = analyzer.list_all_types(directory)
            print(json.dumps(types, indent=2))

        else:
            print(f"Unknown command: {command}")
            print(__doc__)
            sys.exit(1)

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
