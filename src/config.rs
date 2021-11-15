use std::{path::PathBuf, str::FromStr, sync::Arc};

use anyhow::anyhow;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    pub const ALL_FORMATS: &'static [&'static str] = &["text", "json"];
}

impl FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(anyhow!("Unknown output format: {}", s)),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AnalyzeTarget {
    Types,
    Values,
    All,
}

impl AnalyzeTarget {
    pub const ALL_TARGETS: &'static [&'static str] = &["types", "values", "all"];

    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            AnalyzeTarget::Types => "types",
            AnalyzeTarget::Values => "values",
            AnalyzeTarget::All => "all",
        }
    }
}

impl FromStr for AnalyzeTarget {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "types" => Ok(Self::Types),
            "values" => Ok(Self::Values),
            "all" => Ok(Self::All),
            _ => Err(anyhow!("Unknown analyze target: {}", s)),
        }
    }
}

pub struct Config {
    pub root: Arc<PathBuf>,
    pub format: OutputFormat,

    pub analyze_target: AnalyzeTarget,
    pub ignored_folders: Vec<PathBuf>,
}
