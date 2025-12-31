use std::path::{Path, PathBuf};

use crate::core::engine::MelonProject;

pub struct ProjectLoader {

}

impl ProjectLoader {
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
}