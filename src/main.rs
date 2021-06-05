use std::time::Instant;

mod dependency_graph;

mod analysis;
mod config;
mod package_json;
mod parsing;
mod reporting;

use analysis::find_unused_exports;
use reporting::report_unused_dependencies;
use structopt::StructOpt;

use crate::analysis::find_unused_dependencies;
use crate::analysis::resolve_module_imports;
use crate::config::Config;
use crate::config::Opts;
use crate::package_json::find_and_read_package_json;
use crate::parsing::parse_all_modules;
use crate::reporting::report_unused_exports;

fn main() -> anyhow::Result<()> {
    let opts = Opts::from_args();
    let config = Config::from_opts(opts);

    let _timer = ScopedTimer::new("Total");

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

        let package_json = find_and_read_package_json(&config.root)?;

        match package_json {
            Some(package_json) => Some(find_unused_dependencies(&modules, &package_json, &config)),
            None => {
                println!("WARNING: Failed to find package.json, skipping dependency analysis.");
                None
            }
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
