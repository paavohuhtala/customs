use std::path::PathBuf;

use structopt::StructOpt;

use customs_analysis::module_visitor::ModuleVisitor;
use customs_analysis::parsing::module_from_file;
use swc_ecma_visit::Visit;

#[derive(StructOpt)]
#[structopt(version = "0.1", author = "Paavo Huhtala <paavo.huhtala@gmail.com>")]
struct Args {
    target_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::from_args();

    let (_, module) = module_from_file(
        &args.target_file,
        customs_analysis::dependency_graph::ModuleKind::TS,
    )?;

    println!("{:#?}", module);

    let mut analyzer = ModuleVisitor::new();
    analyzer.visit_module(&module, &module);

    println!("{:#?}", analyzer);

    Ok(())
}
