//! Source file management for LSP.
//!
//! Manages file identity, URI mapping, and versioned text buffers.
//! This is the single source of truth for file contents in the LSP.

use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

/// Stable numeric file identifier.
/// 
/// FileIds are stable across file renames and can be used as keys
/// in internal data structures. They map 1:1 with URIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

/// Versioned source file with metadata.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub uri: Url,
    pub text: String,
    pub version: i32,
}

/// Source manager tracking all open files.
/// 
/// Maps URIs to FileIds and manages versioned text buffers.
/// This is owned by the compiler session and used by the LSP.
#[derive(Debug)]
pub struct SourceManager {
    files: HashMap<Url, FileId>,
    file_data: HashMap<FileId, SourceFile>,
    next_file_id: u32,
}

impl SourceManager {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            file_data: HashMap::new(),
            next_file_id: 0,
        }
    }

    /// Get or create a FileId for a URI.
    pub fn get_or_create_file_id(&mut self, uri: &Url) -> FileId {
        if let Some(&file_id) = self.files.get(uri) {
            return file_id;
        }

        let file_id = FileId(self.next_file_id);
        self.next_file_id += 1;

        self.files.insert(uri.clone(), file_id);
        self.file_data.insert(file_id, SourceFile {
            id: file_id,
            uri: uri.clone(),
            text: String::new(),
            version: 0,
        });

        file_id
    }

    /// Get FileId for a URI, returning None if not found.
    pub fn get_file_id(&self, uri: &Url) -> Option<FileId> {
        self.files.get(uri).copied()
    }

    /// Get URI for a FileId.
    pub fn get_uri(&self, file_id: FileId) -> Option<&Url> {
        self.file_data.get(&file_id).map(|f| &f.uri)
    }

    /// Update file contents and version.
    pub fn update_file(&mut self, uri: &Url, text: String, version: i32) -> FileId {
        let file_id = self.get_or_create_file_id(uri);
        if let Some(file) = self.file_data.get_mut(&file_id) {
            file.text = text;
            file.version = version;
        }
        file_id
    }

    /// Get file contents by FileId.
    pub fn get_file_text(&self, file_id: FileId) -> Option<&str> {
        self.file_data.get(&file_id).map(|f| f.text.as_str())
    }

    /// Get file contents by URI.
    pub fn get_file_text_by_uri(&self, uri: &Url) -> Option<&str> {
        self.get_file_id(uri)
            .and_then(|id| self.get_file_text(id))
    }

    /// Get source file by FileId.
    pub fn get_file(&self, file_id: FileId) -> Option<&SourceFile> {
        self.file_data.get(&file_id)
    }

    /// Remove a file (called on didClose).
    pub fn remove_file(&mut self, uri: &Url) -> Option<FileId> {
        if let Some(file_id) = self.files.remove(uri) {
            self.file_data.remove(&file_id);
            Some(file_id)
        } else {
            None
        }
    }

    /// Get all file IDs.
    pub fn all_file_ids(&self) -> impl Iterator<Item = FileId> + '_ {
        self.file_data.keys().copied()
    }

    /// List all file URIs (for debugging).
    pub fn list_files(&self) -> Vec<String> {
        self.files.keys()
            .map(|uri| uri.to_string())
            .collect()
    }
}

impl Default for SourceManager {
    fn default() -> Self {
        Self::new()
    }
}
