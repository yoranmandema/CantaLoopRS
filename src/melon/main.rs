use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::io::Write;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;

use CantaLoopRS::core::ast::Program;
use CantaLoopRS::core::engine::RunArtifacts;
use CantaLoopRS::core::hir_lowering::HirAst;
use CantaLoopRS::{stdlib, Engine};

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

    println!("Created project '{}' at {}", project_name, project_path.display());
    println!("Next:");
    println!("  cd {}", project_name);
    println!("  melon run");
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

    std::fs::write(
        project_path.join("melon.json"),
        format!(
            r#"{{
  "name": "{}",
  "version": "0.1.0",
  "main": "src/main.mln"
}}"#,
            project_name
        ),
    )?;

    std::fs::write(
        project_path.join("src/main.mln"),
        r#"use std.print;

print("Hello, world! 🍉")!;
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
    let project = match Engine::load_project(project_root) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Project error: {}", e);
            return;
        }
    };

    let mut engine = Engine::new();
    stdlib::load_all_stdlib(&mut engine);

    if let Err(e) = engine.load_project_modules(project_root) {
        eprintln!("Module load warning: {}", e);
    }

    let main_path = project.entry.to_str().unwrap();
    println!("{}", main_path);

    match engine.compile(main_path) {
        Ok(artifacts) => {
            println!("🍉 Compile OK");
            engine.run(artifacts.clone());

            if debug {
                write_ast(project_root, &artifacts.ast);
                write_hir(project_root, &artifacts.hir);
                write_bytecode(project_root, &artifacts);
            }
        }
        Err(err) => {
            eprintln!("❌ Compile error:\n{:#?}", err);
        }
    }
}

fn run_with_watch(project_root: &Path, debug: bool) {
    let src = project_root.join("src");
    let (tx, rx) = channel();

    let mut watcher =
        RecommendedWatcher::new(tx, notify::Config::default()).unwrap();
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

    std::fs::write(path, out).unwrap();
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
