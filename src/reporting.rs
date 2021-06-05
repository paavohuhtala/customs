use std::io::stdout;
use std::io::Write;

use crate::analysis::UnusedExportsResults;
use crate::config::Config;

pub fn report_unused_exports(
    UnusedExportsResults { sorted_exports }: UnusedExportsResults,
    _config: &Config,
) -> anyhow::Result<()> {
    if sorted_exports.is_empty() {
        println!("No unused exports!");
        return Ok(());
    }

    let stdout = stdout();
    let mut stdout = stdout.lock();

    writeln!(stdout, "Unused exports:")?;

    for (name, location) in sorted_exports {
        writeln!(&mut stdout, "  {} - {}", location, name)?;
    }

    stdout.flush()?;

    Ok(())
}

pub fn report_unused_dependencies(mut dependencies: Vec<String>, _config: &Config) {
    dependencies.sort_unstable();

    if dependencies.is_empty() {
        println!("No unused dependencies.");
        return;
    }

    println!("Potentially unused dependencies:");

    for dependency in dependencies {
        println!("  {}", dependency);
    }
}
