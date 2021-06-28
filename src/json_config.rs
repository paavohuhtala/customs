use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::Deserialize;

pub trait JsonConfig {
    fn file_name() -> &'static str;
}

fn find_config_path<Config: JsonConfig>(folder: &Path) -> Option<PathBuf> {
    let mut package_json_path = folder.to_owned();
    package_json_path.push(Config::file_name());

    if package_json_path.is_file() {
        return Some(package_json_path);
    }

    match folder.parent() {
        None => None,
        Some(parent) => find_config_path::<Config>(parent),
    }
}

fn read_config<Config: JsonConfig>(package_json_path: &Path) -> anyhow::Result<Config>
where
    for<'a> Config: Deserialize<'a>,
{
    let file = File::open(package_json_path)
        .with_context(|| format!("Failed to open {}", package_json_path.display()))?;
    let reader = BufReader::new(file);
    let manifest: Config = serde_json::from_reader(reader)
        .with_context(|| format!("Failed to parse {}", package_json_path.display()))?;

    Ok(manifest)
}

pub fn find_and_read_config<Config: JsonConfig>(
    root: &Path,
) -> anyhow::Result<Option<(PathBuf, Config)>>
where
    for<'a> Config: Deserialize<'a>,
{
    let package_json_path = find_config_path::<Config>(root);

    match package_json_path {
        None => Ok(None),
        Some(path) => Ok(Some((path.clone(), read_config(&path)?))),
    }
}
