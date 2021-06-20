use std::path::PathBuf;

use structopt::StructOpt;

use customs_analysis::module_visitor::ModuleVisitor;
use customs_analysis::parsing::parse_module_to_ast;
use swc_ecma_visit::Visit;

#[derive(StructOpt)]
#[structopt(version = "0.1", author = "Paavo Huhtala <paavo.huhtala@gmail.com>")]
struct Args {
    target_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::from_args();

    let (_, module) = parse_module_to_ast(
        &args.target_file,
        customs_analysis::dependency_graph::ModuleKind::TS,
    )?;

    let mut analyzer = ModuleVisitor::new();
    analyzer.visit_module(&module, &module);

    println!("{:#?}", analyzer);

    Ok(())
}
