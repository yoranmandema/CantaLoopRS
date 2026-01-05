//! Native module loader for project-local Rust bindings
//!
//! This module handles discovery, compilation, and loading of native modules
//! from a project's `native/` directory.

use std::path::Path;
use std::process::Command;
use std::fs;

use crate::core::engine::Engine;

/// Load native modules from a project directory into an engine.
///
/// This function:
/// 1. Checks for `native/modules.rs`
/// 2. Compiles it as a dynamic library
/// 3. Loads the library and calls the registration function
///
/// # Arguments
/// * `engine` - The engine to register modules into
/// * `project_root` - The root directory of the project
///
/// # Returns
/// * `Ok(())` if native modules were loaded successfully (or no native directory exists)
/// * `Err` if there was an error loading native modules
pub fn load_native_modules(engine: &mut Engine, project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let native_dir = project_root.join("native");
    
    // Check if native directory exists
    if !native_dir.exists() {
        return Ok(()); // No native modules - this is fine
    }

    // Check for modules.rs
    let modules_rs = native_dir.join("modules.rs");
    if !modules_rs.exists() {
        return Ok(()); // No modules file - this is fine
    }

    // Compile and load native modules
    compile_and_load_native_modules(engine, &native_dir, project_root)?;
    
    Ok(())
}

/// Compile native modules and load them as a dynamic library
fn compile_and_load_native_modules(
    engine: &mut Engine,
    native_dir: &Path,
    project_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check if we need to create a Cargo.toml for the native crate
    let cargo_toml = native_dir.join("Cargo.toml");
    let needs_cargo_toml = !cargo_toml.exists();
    
    if needs_cargo_toml {
        // Create a minimal Cargo.toml for the native modules
        let project_name = project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        
        // For standalone projects, users need to configure cantaloop dependency themselves
        // We provide a template that they can customize
        let cargo_content = format!(
            r#"# Native modules for your Melon project
# 
# To use this, you need to add cantaloop as a dependency.
# Options:
#   1. If cantaloop is published: cantaloop = "0.1.0"
#   2. If using from local path: cantaloop = {{ path = "../../path/to/cantaloop" }}
#   3. If using from git: cantaloop = {{ git = "https://github.com/..." }}

[package]
name = "{}-native"
version = "0.1.0"
edition = "2021"

[lib]
name = "native_modules"
path = "lib.rs"
crate-type = ["cdylib"]

[dependencies]
# TODO: Uncomment and configure one of these options:
# cantaloop = "0.1.0"  # If published to crates.io
# cantaloop = {{ path = "../../cantaloop" }}  # If using local path
# cantaloop = {{ git = "https://github.com/yourusername/cantaloop" }}  # If using git
lazy_static = "1.5.0"
"#,
            project_name
        );
        
        fs::write(&cargo_toml, cargo_content)?;
    }
    
    // Check if we need to create lib.rs
    let lib_rs = native_dir.join("lib.rs");
    if !lib_rs.exists() {
        // Create lib.rs that re-exports modules.rs
        // The registration function should be in modules.rs
        let lib_content = r#"// Auto-generated lib.rs for native modules
pub mod modules;
pub use modules::*;
"#;
        fs::write(&lib_rs, lib_content)?;
    }
    
    // Compile the native modules
    // Build from the native directory so target/ is created there
    // Pass through stdout/stderr so users can see cargo build progress
    eprintln!("Compiling native modules...");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(&cargo_toml)
        .arg("--lib")
        .arg("--release")
        .current_dir(&native_dir)
        .status()?;
    
    if !status.success() {
        eprintln!("");
        eprintln!("Warning: Failed to compile native modules.");
        eprintln!("Native modules will not be available.");
        eprintln!("");
        eprintln!("Tip: Make sure cantaloop dependency is configured in native/Cargo.toml");
        eprintln!("     See native/Cargo.toml for configuration options.");
        return Ok(()); // Don't fail the whole run, just skip native modules
    }
    
    eprintln!("Native modules compiled successfully.");
    
    // Find the compiled library
    // The library name is "native_modules" (from Cargo.toml [lib] name)
    let lib_name = "native_modules";
    
    // Try to find the library in common locations
    // When building from native/, cargo creates target/ in the native/ directory
    let possible_locations = vec![
        // Built in native directory's target (when cargo build is run from native/)
        native_dir.join("target").join("release"),
        // Built in native directory's parent target (if in a workspace)
        native_dir.parent().unwrap().join("target").join("release"),
        // Built in project root's target (fallback)
        project_root.join("target").join("release"),
    ];
    
    let lib_file = possible_locations
        .iter()
        .find_map(|base| {
            #[cfg(target_os = "windows")]
            {
                let path = base.join(format!("{}.dll", lib_name));
                if path.exists() { Some(path) } else { None }
            }
            #[cfg(not(target_os = "windows"))]
            {
                let path = base.join(format!("lib{}.so", lib_name));
                if path.exists() { Some(path) } else { None }
            }
        });
    
    let lib_file = match lib_file {
        Some(path) => path,
        None => {
            eprintln!("Warning: Compiled native module library not found.");
            eprintln!("  Looked for: {}.dll (Windows) or lib{}.so (Unix)", lib_name, lib_name);
            eprintln!("  In locations:");
            for loc in &possible_locations {
                eprintln!("    - {:?}", loc);
            }
            return Ok(()); // Don't fail, just skip
        }
    };
    
    // Load the library and call the registration function
    unsafe {
        use libloading::{Library, Symbol};
        
        let lib = Library::new(&lib_file)?;
        
        // Get the registration function
        let register_fn: Symbol<'_, extern "C" fn(*mut Engine)> = lib.get(b"register_native_modules")?;
        
        // Call it to register modules
        register_fn(engine as *mut Engine);
        
        // Leak the library so it stays loaded for the duration of the program
        std::mem::forget(lib);
    }
    
    Ok(())
}
