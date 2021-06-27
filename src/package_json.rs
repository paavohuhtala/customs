use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::Deserialize;
use serde_json;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackageJson {
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    #[serde(default)]
    pub dev_dependencies: HashMap<String, String>,

    pub main: Option<String>,
    pub style: Option<String>,
}

fn find_package_json_path(folder: &Path) -> Option<PathBuf> {
    let mut package_json_path = folder.to_owned();
    package_json_path.push("package.json");

    if package_json_path.is_file() {
        return Some(package_json_path);
    }

    match folder.parent() {
        None => None,
        Some(parent) => find_package_json_path(parent),
    }
}

pub fn read_package_json(package_json_path: &Path) -> anyhow::Result<PackageJson> {
    let file = File::open(package_json_path).with_context(|| "Failed to open package.json")?;
    let reader = BufReader::new(file);
    let manifest: PackageJson =
        serde_json::from_reader(reader).with_context(|| "Failed to parse package.json")?;

    Ok(manifest)
}

pub fn find_and_read_package_json(root: &Path) -> anyhow::Result<Option<PackageJson>> {
    let package_json_path = find_package_json_path(root);

    match package_json_path {
        None => Ok(None),
        Some(path) => Ok(Some(read_package_json(&path)?)),
    }
}
