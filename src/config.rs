use std::{path::PathBuf, str::FromStr};

use anyhow::anyhow;
use structopt::StructOpt;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OutputFormat {
    Clean,
    Compact,
}

impl OutputFormat {
    pub const ALL_FORMATS: &'static [&'static str] = &["clean", "compact"];
}

impl FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "clean" => Ok(Self::Clean),
            "compact" => Ok(Self::Compact),
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

#[derive(StructOpt)]
#[structopt(version = "0.1", author = "Paavo Huhtala <paavo.huhtala@gmail.com>")]
pub struct Opts {
    target_dir: PathBuf,
    #[structopt(short, long, default_value = "compact", possible_values = OutputFormat::ALL_FORMATS)]
    format: OutputFormat,

    #[structopt(short, long, default_value = "all", possible_values = AnalyzeTarget::ALL_TARGETS)]
    analyze: AnalyzeTarget,
}

pub struct Config {
    pub root: PathBuf,
    pub format: OutputFormat,

    pub analyze_target: AnalyzeTarget,
}

impl Config {
    pub fn from_opts(opts: Opts) -> Config {
        Config {
            root: opts.target_dir,
            format: opts.format,
            analyze_target: opts.analyze,
        }
    }
}
