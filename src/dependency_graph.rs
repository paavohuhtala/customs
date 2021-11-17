use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    fmt::Display,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use relative_path::RelativePath;
use swc_atoms::JsWord;

use crate::config::AnalyzeTarget;

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub struct NormalizedModulePath(PathBuf);

impl NormalizedModulePath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }
}

impl Deref for NormalizedModulePath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(PartialEq, Eq, Hash, Debug, PartialOrd, Ord, Clone)]
pub enum ExportName {
    Named(JsWord),
    Default,
}

impl ExportName {
    pub fn named(name: impl Into<JsWord>) -> Self {
        Self::Named(name.into())
    }
}

impl Display for ExportName {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ExportName::Named(name) => write!(f, "{}", name),
            ExportName::Default => write!(f, "default"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleSourceAndLine {
    path: Arc<PathBuf>,
    zero_based_line: usize,
}

impl ModuleSourceAndLine {
    pub fn new(path: Arc<PathBuf>, zero_based_line: usize) -> ModuleSourceAndLine {
        ModuleSourceAndLine {
            path,
            zero_based_line,
        }
    }

    #[cfg(test)]
    pub fn new_mock() -> ModuleSourceAndLine {
        ModuleSourceAndLine::new(Arc::new(PathBuf::from("mock.ts")), 0)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn line(&self) -> usize {
        self.zero_based_line + 1
    }
}

impl Display for ModuleSourceAndLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.path.display(), self.line())
    }
}

#[derive(Debug)]
pub struct Export {
    pub usage: Cell<Usage>,
    pub kind: ExportKind,
    pub visibility: Visibility,
    pub source: ModuleSourceAndLine,
}

impl Export {
    pub fn new(kind: ExportKind, visibility: Visibility, source: ModuleSourceAndLine) -> Self {
        Export {
            usage: Cell::default(),
            kind,
            visibility,
            source,
        }
    }

    pub fn is_used(&self) -> bool {
        self.usage.get().is_used()
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Default, Copy, Clone)]
pub struct Usage {
    pub used_locally: bool,
    pub used_externally: bool,
}

impl Usage {
    pub fn is_used(self) -> bool {
        self.used_locally || self.used_externally
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub enum ImportName {
    Named(JsWord),
    Default,
    Wildcard,
}

impl ImportName {
    pub fn named(name: impl Into<JsWord>) -> Self {
        ImportName::Named(name.into())
    }
}

pub struct ModulePath {
    pub root: Arc<PathBuf>,
    pub root_relative: Arc<PathBuf>,
    pub normalized: NormalizedModulePath,
}

pub struct Module {
    pub path: ModulePath,
    pub kind: ModuleKind,
    pub exports: HashMap<ExportName, Export>,
    pub imported_modules: HashMap<NormalizedModulePath, Vec<ImportName>>,
    pub imported_packages: HashSet<String>,
    is_wildcard_imported: Cell<bool>,
}

impl Module {
    pub fn new(path: ModulePath, kind: ModuleKind) -> Module {
        Module {
            path,
            kind,
            exports: HashMap::new(),
            imported_modules: HashMap::new(),
            imported_packages: HashSet::new(),
            is_wildcard_imported: Cell::default(),
        }
    }

    pub fn is_wildcard_imported(&self) -> bool {
        self.is_wildcard_imported.get()
    }

    pub fn mark_wildcard_imported(&self) {
        self.is_wildcard_imported.set(true)
    }

    pub fn add_export(&mut self, name: ExportName, export: Export) {
        self.exports.insert(name, export);
    }

    pub fn imports_mut(&mut self, module_path: NormalizedModulePath) -> &mut Vec<ImportName> {
        self.imported_modules
            .entry(module_path)
            .or_insert_with(Vec::new)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ModuleKind {
    TS,
    TSX,
    DTS,
}

impl ModuleKind {
    pub fn is_declaration(self) -> bool {
        self == ModuleKind::DTS
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ExportKind {
    Type,
    Value,
    Class,
    Enum,
    Unknown,
}

impl ExportKind {
    pub fn matches_analyze_target(self, target: AnalyzeTarget) -> bool {
        matches!(
            (self, target),
            (_, AnalyzeTarget::All)
                | (ExportKind::Class | ExportKind::Enum, _)
                | (ExportKind::Type, AnalyzeTarget::Types)
                | (ExportKind::Value, AnalyzeTarget::Values)
        )
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Visibility {
    Exported,
    ImplicitlyExported,
}

pub fn normalize_module_path(
    project_root: &Path,
    module_path: &Path,
) -> anyhow::Result<NormalizedModulePath> {
    let normalized_path = module_path.strip_prefix(project_root).with_context(|| {
        format!(
            "Failed to convert {:?} to be relative of {:?}",
            module_path, project_root
        )
    })?;

    let folder = normalized_path
        .parent()
        .expect("The file must be in a folder.");

    let file_name = normalized_path
        .file_name()
        .expect("The path must point to a file")
        .to_string_lossy();

    // This is not exactly nice and/or clean, but it is the cleanest I could come up with for handling files like foo.stories.tsx.
    let file_name_without_extension = file_name
        .trim_end_matches(".d.ts")
        .trim_end_matches(".ts")
        .trim_end_matches(".tsx");

    let normalized_path = folder.join(file_name_without_extension);

    Ok(NormalizedModulePath(normalized_path))
}

pub enum NormalizedImportSource {
    Local(NormalizedModulePath),
    Global(String),
}

pub fn resolve_import_source(
    project_root: &Path,
    current_folder: &Path,
    import_source: &str,
) -> anyhow::Result<NormalizedImportSource> {
    if !import_source.starts_with('.') {
        return Ok(NormalizedImportSource::Global(String::from(import_source)));
    }

    let mut absolute_path = RelativePath::new(import_source).to_logical_path(current_folder);

    for ext in ["d.ts", "ts", "tsx"] {
        let with_ext = absolute_path.clone().with_extension(ext);
        if with_ext.is_file() {
            return normalize_module_path(project_root, &with_ext)
                .map(NormalizedImportSource::Local);
        }
    }

    absolute_path.push("index.ts");
    normalize_module_path(project_root, &absolute_path).map(NormalizedImportSource::Local)
}
