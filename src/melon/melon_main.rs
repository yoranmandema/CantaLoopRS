use std::path::Path;
use CantaLoopRS::{Engine, stdlib};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: melon <command> [args...]");
        eprintln!("\nCommands:");
        eprintln!("  run    Run the main file from a melon project");
        std::process::exit(1);
    }
    
    let command = &args[1];
    
    match command.as_str() {
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

/// Find the project root by looking for melon.json in the current directory and parent directories
fn find_project_root(start: &Path) -> Option<std::path::PathBuf> {
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

