use std::time::Instant;

mod dependency_graph;

mod analysis;
mod config;
mod parsing;
mod reporting;

use structopt::StructOpt;

use crate::analysis::analyze_imports;
use crate::config::Config;
use crate::config::Opts;
use crate::parsing::parse_all_modules;
use crate::reporting::report_unused_dependencies;

fn main() -> anyhow::Result<()> {
    let opts = Opts::from_args();
    let config = Config::from_opts(opts);

    let start_time = Instant::now();

    let modules = parse_all_modules(&config);

    let finished_parse_time = Instant::now();
    let parse_duration = finished_parse_time - start_time;
    println!(
        "Parsed {} modules in {} ms",
        modules.len(),
        parse_duration.as_millis()
    );

    analyze_imports(&modules);

    let resolution_duration = finished_parse_time.elapsed();

    println!("Resolved imports in {} ms", resolution_duration.as_millis());

    report_unused_dependencies(modules, &config)?;

    println!("Finished in {}ms", start_time.elapsed().as_millis());

    Ok(())
}
