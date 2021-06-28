use std::collections::HashMap;

use serde::Deserialize;

use crate::json_config::JsonConfig;

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

impl JsonConfig for PackageJson {
    fn file_name() -> &'static str {
        "package.json"
    }
}
