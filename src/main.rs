use std::{convert::TryFrom, path::PathBuf, time::Instant};

mod dependency_graph;

mod analysis;
mod parsing;
mod reporting;

use structopt::StructOpt;

use crate::analysis::analyze_imports;
use crate::parsing::parse_all_modules;
use crate::reporting::{report_unused_dependencies, OutputFormat};

#[derive(StructOpt)]
#[structopt(version = "0.1", author = "Paavo Huhtala <paavo.huhtala@gmail.com>")]
struct Opts {
    target_dir: String,
    #[structopt(short, long, default_value = "compact", possible_values = OutputFormat::ALL_FORMATS)]
    format: OutputFormat,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::from_args();
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

    report_unused_dependencies(modules, opts.format);

    println!("Finished in {}ms", start_time.elapsed().as_millis());

    Ok(())
}
