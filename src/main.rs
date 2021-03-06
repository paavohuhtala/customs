use std::{path::PathBuf, sync::Arc, time::Instant};

use customs_analysis::{
    analysis::{find_unused_dependencies, find_unused_exports, resolve_module_imports},
    config::{AnalyzeTarget, Config, OutputFormat},
    json_config::find_and_read_config,
    package_json::PackageJson,
    parsing::parse_all_modules,
    reporting::{report_unused_dependencies, report_unused_exports},
    tsconfig::TsConfig,
};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(version = "0.1", author = "Paavo Huhtala <paavo.huhtala@gmail.com>")]
struct Opts {
    target_dir: PathBuf,

    // Disabled since only one foramt is implemented right now
    //#[structopt(short, long, default_value = "text", possible_values = OutputFormat::ALL_FORMATS)]
    //format: OutputFormat,
    #[structopt(short, long, default_value = "all", possible_values = AnalyzeTarget::ALL_TARGETS)]
    analyze: AnalyzeTarget,
}

impl Opts {
    pub fn into_config(self) -> Config {
        Config {
            root: Arc::new(self.target_dir),
            format: OutputFormat::Text,
            analyze_target: self.analyze,
            ignored_folders: Vec::new(),
        }
    }
}

fn main() -> anyhow::Result<()> {
    let mut config = Opts::from_args().into_config();

    let _timer = ScopedTimer::new("Total");

    let tsconfig = find_and_read_config::<TsConfig>(&config.root)?;

    if let Some((path, tsconfig)) = tsconfig {
        let mut roots = tsconfig.normalized_type_roots(&path);
        config.ignored_folders.append(&mut roots);
    }

    let modules = {
        let _timer = ScopedTimer::new("Parsing");
        let modules = parse_all_modules(&config);
        println!("Parsed {} modules", modules.len());
        modules
    };

    {
        let _timer = ScopedTimer::new("Import resolution");
        resolve_module_imports(&modules);
    }

    let unused_dependencies = {
        let _timer = ScopedTimer::new("Unused dependency analysis");

        let package_json = find_and_read_config::<PackageJson>(&config.root)?;

        if let Some((_, package_json)) = package_json {
            Some(find_unused_dependencies(&modules, &package_json, &config))
        } else {
            println!("WARNING: Failed to find package.json, skipping dependency analysis.");
            None
        }
    };

    let unused_exports = {
        let _timer = ScopedTimer::new("Unused exports analysis");
        find_unused_exports(modules, &config)
    };

    report_unused_exports(unused_exports, &config)?;

    if let Some(dependencies) = unused_dependencies {
        report_unused_dependencies(dependencies, &config);
    }

    Ok(())
}

struct ScopedTimer {
    name: &'static str,
    started_at: Instant,
}

impl ScopedTimer {
    pub fn new(name: &'static str) -> Self {
        ScopedTimer {
            name,
            started_at: Instant::now(),
        }
    }
}

impl Drop for ScopedTimer {
    fn drop(&mut self) {
        println!("{}: {}ms", self.name, self.started_at.elapsed().as_millis());
    }
}
