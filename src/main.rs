use std::{convert::TryFrom, path::PathBuf, time::Instant};

use clap::{AppSettings, Clap};

mod dependency_graph;

mod analysis;
mod parsing;
mod reporting;

use crate::analysis::analyze_imports;
use crate::parsing::parse_all_modules;
use crate::reporting::report_unused_dependencies;

#[derive(Clap)]
#[clap(version = "0.1", author = "Paavo Huhtala <paavo.huhtala@gmail.com>")]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    target_dir: String,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let root = PathBuf::try_from(opts.target_dir)?;

    let start_time = Instant::now();

    let modules = parse_all_modules(&root);

    let finished_parse_time = Instant::now();
    let parse_duration = finished_parse_time - start_time;
    println!(
        "Parsed {} modules in {} ms",
        modules.len(),
        parse_duration.as_millis()
    );

    let mut total_imports = 0;

    analyze_imports(&modules, &mut total_imports);

    let resolution_duration = finished_parse_time.elapsed();

    println!(
        "Resolved {} imports in {} ms",
        total_imports,
        resolution_duration.as_millis()
    );

    report_unused_dependencies(modules);

    println!("Finished in {}ms", start_time.elapsed().as_millis());

    Ok(())
}
