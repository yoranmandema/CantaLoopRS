use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Instant;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use cantaloop::core::projectLoader::ProjectLoader;

use cantaloop::core::ast::Program;
use cantaloop::core::engine::RunArtifacts;
use cantaloop::core::hir_lowering::HirAst;
use cantaloop::{stdlib, Engine};

fn main() {
    env::set_var("RUST_BACKTRACE", "1");

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: melon <command> [args...]");
        eprintln!("Commands:");
        eprintln!("  new [name]");
        eprintln!("  run [--watch|-w] [--debug]");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "new" => create_new_project_cmd(&args),
        "run" => run_project_cmd(&args),
        other => {
            eprintln!("Unknown command: {}", other);
            std::process::exit(1);
        }
    }
}

/* ============================
Project creation
============================ */

fn create_new_project_cmd(args: &[String]) {
    let name = if args.len() >= 3 {
        args[2].clone()
    } else {
        print!("project name > ");
        read_line()
    };

    finish_project_creation(name);
}

fn finish_project_creation(project_name: String) {
    let cwd = env::current_dir().expect("cwd");
    let project_path = cwd.join(&project_name);

    if let Err(e) = create_new_project(&project_path, &project_name) {
        eprintln!("Error creating project: {}", e);
        std::process::exit(1);
    }

    println!(
        "Created project '{}' at {}",
        project_name,
        project_path.display()
    );
    println!("Next:");
    println!("  cd {}", project_name);
    println!("  melon run");
    println!();
    println!("💡 Tip: Edit native/modules.rs to add custom Rust functions!");
    println!("Happy coding! 🍉");
}

fn create_new_project(
    project_path: &Path,
    project_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if project_path.exists() {
        return Err("directory already exists".into());
    }

    std::fs::create_dir_all(project_path.join("src"))?;
    std::fs::create_dir_all(project_path.join("tests"))?;
    std::fs::create_dir_all(project_path.join("native"))?;

    std::fs::write(
        project_path.join("melon.json"),
        format!(
            r#"{{
  "name": "{}",
  "version": "0.1.0",
  "main": "main.mln"
}}"#,
            project_name
        ),
    )?;

    std::fs::write(
        project_path.join("src/main.mln"),
        r#"use print from std;

print("Hello, world! 🍉")!;
"#,
    )?;

    // Create native/Cargo.toml template
    std::fs::write(
        project_path.join("native/Cargo.toml"),
        format!(
            r#"# Native modules for your Melon project
#
# This Cargo.toml defines your native Rust modules as a separate crate.
# Configure the cantaloop dependency based on your setup:
#
# Option 1: If cantaloop is published to crates.io:
#   cantaloop = "0.1.0"
#
# Option 2: If using from a local path (e.g., during development):
#   cantaloop = {{ path = "../../cantaloop" }}
#
# Option 3: If using from git:
#   cantaloop = {{ git = "https://github.com/yourusername/cantaloop" }}

[package]
name = "{}-native"
version = "0.1.0"
edition = "2021"

[lib]
name = "native_modules"
path = "lib.rs"
crate-type = ["cdylib"]

[dependencies]
# Configure cantaloop dependency - uncomment and adjust as needed:
# cantaloop = "0.1.0"  # If published
# cantaloop = {{ path = "../../cantaloop" }}  # If local path
# cantaloop = {{ git = "https://github.com/yourusername/cantaloop" }}  # If git
lazy_static = "1.5.0"
"#,
            project_name
        ),
    )?;

    // Create native/lib.rs
    std::fs::write(
        project_path.join("native/lib.rs"),
        r#"// Auto-generated lib.rs for native modules
#[macro_use]
extern crate lazy_static;

pub mod modules;
pub use modules::*;
"#,
    )?;

    // Create native/modules.rs template
    std::fs::write(
        project_path.join("native/modules.rs"),
        r#"// Native Rust modules for your Melon project
//
// This file contains native Rust functions that can be used in your Melon code.
// Use the melon_module! macro to define modules declaratively.
//
// Example:
//   use mymath.hypot;
//   let result = hypot(3, 4);  // 5.0

use cantaloop::core::engine::StdModule;
use cantaloop::core::vm::Value;
use cantaloop::melon_module;

// Define your native modules here
lazy_static::lazy_static! {
    pub static ref MYMATH_MODULE: StdModule = melon_module! {
        module mymath {
            fn hypot(a: num, b: num) -> num {
                |args, _heap| {
                    let a = args[0].as_number().expect("expected number");
                    let b = args[1].as_number().expect("expected number");
                    Value::number((a*a + b*b).sqrt())
                }
            }
        }
    };
}

// Export a registration function that can be called from the melon runtime
#[no_mangle]
pub extern "C" fn register_native_modules(engine: *mut cantaloop::core::engine::Engine) {
    unsafe {
        if let Some(engine_ref) = engine.as_mut() {
            // Register modules directly (can't use const array with lazy_static)
            engine_ref.register_module(&*MYMATH_MODULE, "");
        }
    }
}
"#,
    )?;

    // Create native/README.md with instructions
    std::fs::write(
        project_path.join("native/README.md"),
        r#"# Native Modules

This directory contains native Rust modules for your Melon project.

## Quick Start

1. **Configure the dependency**: Edit `Cargo.toml` and uncomment/configure the `cantaloop` dependency
   - If cantaloop is published: `cantaloop = "0.1.0"`
   - If using local path: `cantaloop = { path = "../../cantaloop" }`
   - If using git: `cantaloop = { git = "https://github.com/..." }`

2. **Define your modules**: Edit `modules.rs` to add your native functions using the `melon_module!` macro

3. **Run your project**: `melon run` will automatically compile and load your native modules

## Example Usage in Melon

```melon
use hypot from mymath;

let result = hypot(3, 4);  // 5.0
```

## How It Works

1. Configure `cantaloop` dependency in `Cargo.toml`
2. Define modules in `modules.rs` using the `melon_module!` macro
3. The `register_native_modules` function exports them for the melon runtime
4. Run `melon run` - it automatically compiles and loads your native modules
5. Import and use them in your Melon code

## Adding More Modules

Just add more module definitions to `modules.rs` and register them in `register_native_modules`:

```rust
lazy_static::lazy_static! {
    pub static ref MYUTILS_MODULE: StdModule = melon_module! {
        module myutils {
            // ... functions ...
        }
    };
}

#[no_mangle]
pub extern "C" fn register_native_modules(engine: *mut cantaloop::core::engine::Engine) {
    unsafe {
        if let Some(engine_ref) = engine.as_mut() {
            engine_ref.register_module(&*MYMATH_MODULE, "");
            engine_ref.register_module(&*MYUTILS_MODULE, "");  // Add more here
        }
    }
}
```

See NATIVE_MODULES.md for detailed documentation.
"#,
    )?;

    Ok(())
}

/* ============================
Run / Watch
============================ */

fn run_project_cmd(args: &[String]) {
    let watch = args.iter().any(|a| a == "--watch" || a == "-w");
    let debug = args.iter().any(|a| a == "--debug");

    let cwd = env::current_dir().expect("cwd");
    let project_root = find_project_root(&cwd).unwrap_or_else(|| {
        eprintln!("No melon.json found");
        std::process::exit(1);
    });

    if watch {
        run_with_watch(&project_root, debug);
    } else {
        run_once(&project_root, debug);
    }
}

fn run_once(project_root: &Path, debug: bool) {
    let project = match ProjectLoader::load_project(project_root) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Project error: {}", e);
            return;
        }
    };

    let mut engine = Engine::new();
    stdlib::load_stdlib_runtime(&mut engine);
    
    // Load project-native Rust bindings (if any)
    // This must happen before wrapping engine in Arc, as it mutates the engine
    if let Err(e) = ProjectLoader::load_native_modules(&mut engine, project_root) {
        eprintln!("Warning: Failed to load project native modules: {}", e);
    }

    let main_path = project.entry.to_str().unwrap();
    println!("{}", main_path);

    // Wrap Engine in Arc so it can be passed to VM
    let engine_arc = Arc::new(engine);
    match engine_arc.compile_with_project(main_path, Some(project_root)) {
        Ok(artifacts) => {
            println!("🍉 Compile OK");

            // Write debug output BEFORE running, so it's available even if execution panics
            if debug {
                write_ast(project_root, &artifacts.ast);
                write_hir(project_root, &artifacts.hir);
                write_bytecode(project_root, &artifacts);
            }

            let start = Instant::now();

            engine_arc.run(artifacts.clone());

            let elapsed = start.elapsed();
            println!("⏱️ Total run time: {:.2?}", elapsed);
        }
        Err(err) => {
            eprintln!("❌ Compile error:\n{:#?}", err);
        }
    }
}

fn run_with_watch(project_root: &Path, debug: bool) {
    let src = project_root.join("src");
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default()).unwrap();
    watcher.watch(&src, RecursiveMode::Recursive).unwrap();

    loop {
        clear_screen();
        println!("🔁 Rebuilding…\n");
        run_once(project_root, debug);
        let _ = rx.recv(); // wait for change
    }
}

/* ============================
Debug output
============================ */

fn write_ast(project_root: &Path, ast: &Program) {
    write_json(project_root, "ast.json", ast);
}

fn write_hir(project_root: &Path, hir: &HirAst) {
    write_json(project_root, "hir.json", hir);
}

fn write_bytecode(project_root: &Path, artifacts: &RunArtifacts) {
    let dir = ensure_debug_dir(project_root);
    let path = dir.join("bytecode.txt");

    let mut out = String::new();

    let mut ids: Vec<u32> = artifacts.functions.keys().cloned().collect();
    ids.sort_unstable();

    for id in ids {
        out.push_str(&format!("\n[Function {}]\n", id));
        for (i, op) in artifacts.functions[&id].iter().enumerate() {
            out.push_str(&format!("  {:04}: {:?}\n", i, op));
        }
    }

    out.push_str("\n[Main]\n");
    for (i, op) in artifacts.main.iter().enumerate() {
        out.push_str(&format!("  {:04}: {:?}\n", i, op));
    }

    if let Err(e) = std::fs::write(&path, out) {
        eprintln!("Failed to write bytecode.txt to {:?}: {}", path, e);
    } else {
        println!("Debug: bytecode written to {:?}", path);
    }
}

fn write_json<T: Serialize>(project_root: &Path, name: &str, value: &T) {
    let dir = ensure_debug_dir(project_root);
    let path = dir.join(name);
    let json = serde_json::to_string_pretty(value).unwrap();
    std::fs::write(path, json).unwrap();
}

fn ensure_debug_dir(project_root: &Path) -> PathBuf {
    let dir = project_root.join(".melon/debug");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/* ============================
Utils
============================ */

fn read_line() -> String {
    let mut s = String::new();
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_line(&mut s).unwrap();
    s.trim().to_string()
}

fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join("melon.json").exists() {
            return Some(cur);
        }
        cur = cur.parent()?.to_path_buf();
    }
}
