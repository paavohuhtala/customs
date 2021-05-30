use std::{
    borrow::Cow,
    cell::Cell,
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use relative_path::RelativePath;
use string_interner::{symbol::SymbolU32, StringInterner};

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub struct NormalizedModulePath(SymbolU32);

impl NormalizedModulePath {
    pub fn resolve<'a>(&self, interner: &'a StringInterner) -> &'a str {
        interner
            .resolve(self.0)
            .expect("Should always exist in cache")
    }
}

/*impl Deref for NormalizedModulePath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}*/

#[derive(PartialEq, Eq, Hash, Debug, Clone, PartialOrd, Ord)]
pub enum ExportName<'a> {
    Named(Cow<'a, str>),
    Default,
}

impl ExportName<'_> {
    pub fn get_name(&self) -> &Cow<'_, str> {
        match self {
            ExportName::Named(name) => name,
            ExportName::Default => &Cow::Borrowed("default"),
        }
    }
}

impl Display for ExportName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportName::Named(name) => write!(f, "{}", name),
            ExportName::Default => write!(f, "default"),
        }
    }
}

#[derive(Debug)]
pub struct ModuleSourceAndLine {
    path: Arc<PathBuf>,
    zero_based_line: usize,
}

impl ModuleSourceAndLine {
    pub fn new(path: Arc<PathBuf>, zero_based_line: usize) -> ModuleSourceAndLine {
        ModuleSourceAndLine {
            path: path.clone(),
            zero_based_line,
        }
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
    pub location: ModuleSourceAndLine,
}

impl Export {
    pub fn new(kind: ExportKind, visibility: Visibility, location: ModuleSourceAndLine) -> Self {
        Export {
            usage: Default::default(),
            kind,
            visibility,
            location,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Default, Copy, Clone)]
pub struct Usage {
    pub used_locally: bool,
    pub used_externally: bool,
}

impl Usage {
    pub fn is_used(&self) -> bool {
        self.used_locally || self.used_externally
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub enum Import {
    Named(String),
    Default,
    Wildcard,
}

pub struct Module<'a> {
    pub kind: ModuleKind,
    pub normalized_path: NormalizedModulePath,
    pub exports: HashMap<ExportName<'a>, Export>,
    pub imported_modules: HashMap<NormalizedModulePath, Vec<Import>>,
    pub source: Arc<PathBuf>,
    is_wildcard_imported: Cell<bool>,
}

impl<'a> Module<'a> {
    pub fn new(
        source_path: Arc<PathBuf>,
        normalized_path: NormalizedModulePath,
        kind: ModuleKind,
    ) -> Module<'a> {
        Module {
            kind,
            source: source_path,
            normalized_path,
            exports: HashMap::new(),
            imported_modules: HashMap::new(),
            is_wildcard_imported: Default::default(),
        }
    }

    pub fn is_wildcard_imported(&self) -> bool {
        self.is_wildcard_imported.get()
    }

    pub fn mark_wildcard_imported(&self) {
        self.is_wildcard_imported.set(true)
    }

    pub fn add_export(&mut self, (name, export): (ExportName<'a>, Export)) {
        self.exports.insert(name, export);
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
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Visibility {
    Exported,
    ImplicitlyExported,
}

pub fn normalize_module_path(
    project_root: &Path,
    module_path: &Path,
    interner: &mut StringInterner,
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

    let joined = folder.join(file_name_without_extension);
    let normalized_path = joined.to_string_lossy();
    let symbol = interner.get_or_intern(normalized_path);

    Ok(NormalizedModulePath(symbol))
}

pub fn resolve_import_path(
    project_root: &Path,
    current_folder: &Path,
    import_source: &str,
    interner: &mut StringInterner,
) -> anyhow::Result<NormalizedModulePath> {
    let mut absolute_path = RelativePath::new(import_source).to_logical_path(current_folder);

    for ext in ["d.ts", "ts", "tsx"] {
        let with_ext = absolute_path.clone().with_extension(ext);
        if with_ext.is_file() {
            return normalize_module_path(project_root, &with_ext, interner);
        }
    }

    absolute_path.push("index.ts");
    normalize_module_path(project_root, &absolute_path, interner)
}
