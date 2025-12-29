use std::path::{Path, PathBuf};
use CantaLoopRS::{Engine, stdlib};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: melon <command> [args...]");
        eprintln!("\nCommands:");
        eprintln!("  new <name>    Create a new melon project");
        eprintln!("  run           Run the main file from a melon project");
        std::process::exit(1);
    }
    
    let command = &args[1];
    
    match command.as_str() {
        "new" => {
            if args.len() < 3 {
                eprintln!("Error: 'new' command requires a project name");
                eprintln!("Usage: melon new <project-name>");
                std::process::exit(1);
            }
            
            let project_name = &args[2];
            let current_dir = std::env::current_dir().expect("Failed to get current directory");
            let project_path = current_dir.join(project_name);
            
            if let Err(e) = create_new_project(&project_path, project_name) {
                eprintln!("Error creating project: {}", e);
                std::process::exit(1);
            }
            
            println!("Created new melon project '{}' at {}", project_name, project_path.display());
            println!("\nTo get started:");
            println!("  cd {}", project_name);
            println!("  melon run");
        }
        "run" => {
            // Find melon.json in current directory or parent directories
            let current_dir = std::env::current_dir().expect("Failed to get current directory");
            let project_root = find_project_root(&current_dir)
                .unwrap_or_else(|| {
                    eprintln!("Error: No melon.json found. Are you in a melon project?");
                    std::process::exit(1);
                });
            
            // Load project configuration
            let project = match Engine::load_project(&project_root) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Error loading project: {}", e);
                    std::process::exit(1);
                }
            };
            
            // Create engine and load stdlib
            let mut engine = Engine::new();
            stdlib::load_all_stdlib(&mut engine);
            
            // Load all modules from the project
            if let Err(e) = engine.load_project_modules(&project_root) {
                eprintln!("Warning: Failed to load some modules: {}", e);
            }
            
            // Run the main file
            let main_path = project.entry.to_str().expect("Invalid main file path");
            engine.run(main_path);
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Run 'melon' for usage information");
            std::process::exit(1);
        }
    }
}

/// Create a new melon project at the specified path
fn create_new_project(project_path: &Path, project_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Check if directory already exists
    if project_path.exists() {
        return Err(format!("Directory '{}' already exists", project_path.display()).into());
    }
    
    // Create project directory
    std::fs::create_dir_all(project_path)?;
    
    // Create src directory
    let src_dir = project_path.join("src");
    std::fs::create_dir_all(&src_dir)?;
    
    // Create tests directory
    let tests_dir = project_path.join("tests");
    std::fs::create_dir_all(&tests_dir)?;
    
    // Create melon.json
    let melon_json = project_path.join("melon.json");
    let melon_json_content = format!(
        r#"{{
    "name": "{}",
    "version": "0.1.0",
    "description": "A new melon project",
    "main": "main.mln",
    "compiler_options": {{
        "optimize": true
    }}
}}"#,
        project_name
    );
    std::fs::write(&melon_json, melon_json_content)?;
    
    // Create src/main.mln
    let main_mln = src_dir.join("main.mln");
    let main_content = r#"use std.print;

print("Hello, world!")!;
"#;
    std::fs::write(&main_mln, main_content)?;
    
    // Create tests/tests.mln
    let tests_mln = tests_dir.join("tests.mln");
    let tests_content = r#"// TODO: implement test framework
// fn test_example() {
//     // Your tests here
// }
"#;
    std::fs::write(&tests_mln, tests_content)?;
    
    // Create .gitignore
    let gitignore = project_path.join(".gitignore");
    let gitignore_content = "/build\n";
    std::fs::write(&gitignore, gitignore_content)?;
    
    Ok(())
}

/// Find the project root by looking for melon.json in the current directory and parent directories
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    
    loop {
        let melon_json = current.join("melon.json");
        if melon_json.exists() {
            return Some(current);
        }
        
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            return None;
        }
    }
}

