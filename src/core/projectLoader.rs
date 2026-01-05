use std::path::{Path, PathBuf};

use crate::core::engine::MelonProject;
use crate::core::native_module_loader;

pub struct ProjectLoader {

}

impl ProjectLoader {
    /// Load project configuration from melon.json
    pub fn load_project(project_path: &Path) -> Result<MelonProject, std::io::Error> {
        let config_path = project_path.join("melon.json");
        let config_data = std::fs::read_to_string(config_path)?;
        let config: serde_json::Value = serde_json::from_str(&config_data)?;

        let main_file = config["main"].as_str().unwrap_or("main.mln");
        let entry = project_path.join("src").join(main_file);

        let scripts: Vec<PathBuf> = config["scripts"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| project_path.join(v.as_str().unwrap()))
            .collect();

        let deps: Vec<String> = config["dependencies"]
            .as_object()
            .unwrap_or(&serde_json::Map::new())
            .keys()
            .cloned()
            .collect();

        Ok(MelonProject {
            name: config["name"].as_str().unwrap().to_string(),
            entry,
            scripts,
            dependencies: deps,
        })
    }

    /// Load native Rust modules from a project's native/ directory into an engine.
    ///
    /// This is a convenience wrapper around `native_module_loader::load_native_modules`.
    pub fn load_native_modules(engine: &mut crate::core::engine::Engine, project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        native_module_loader::load_native_modules(engine, project_root)
    }
}