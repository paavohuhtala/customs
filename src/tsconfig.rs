use std::path::{Path, PathBuf};

use relative_path::RelativePath;
use serde::Deserialize;

use crate::json_config::JsonConfig;

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
    type_roots: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
    compiler_options: Option<CompilerOptions>,
}

impl JsonConfig for TsConfig {
    fn file_name() -> &'static str {
        "tsconfig.json"
    }
}

impl TsConfig {
    pub fn normalized_type_roots(&self, tsconfig_file_path: &Path) -> Vec<PathBuf> {
        let root_folder = tsconfig_file_path
            .parent()
            .expect("tsconfig.json path should always have a parent");

        match &self.compiler_options {
            Some(CompilerOptions {
                type_roots: Some(roots),
            }) => roots
                .iter()
                .map(|type_root| RelativePath::new(type_root).to_logical_path(root_folder))
                .filter(|path| path.exists())
                .collect(),
            _ => Vec::new(),
        }
    }
}
