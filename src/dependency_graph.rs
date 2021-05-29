use std::{
    borrow::Cow,
    cell::Cell,
    collections::HashMap,
    ops::Deref,
    path::{Path, PathBuf},
};

use anyhow::Context;
use relative_path::RelativePath;

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub struct NormalizedModulePath(PathBuf);

impl Deref for NormalizedModulePath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub enum ExportName<'a> {
    Named(Cow<'a, str>),
    Default,
}

impl ExportName<'_> {
    pub fn get_name(&self) -> &Cow<'_, str> {
        match self {
            ExportName::Named(name) => name,
            ExportName::Default => &Cow::Borrowed("Default"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Export {
    pub usage: Cell<Usage>,
    pub kind: ExportKind,
    pub visibility: Visibility,
}

impl Export {
    pub fn new(kind: ExportKind, visibility: Visibility) -> Self {
        Export {
            usage: Default::default(),
            kind,
            visibility,
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

#[derive(Debug, Clone)]
pub struct Module<'a> {
    pub path: NormalizedModulePath,
    pub exports: HashMap<ExportName<'a>, Export>,
    pub imported_modules: HashMap<NormalizedModulePath, Vec<Import>>,
    is_wildcard_imported: Cell<bool>,
    is_dts: bool,
}

impl<'a> Module<'a> {
    pub fn new(
        path: NormalizedModulePath,
        exports: HashMap<ExportName<'a>, Export>,
        imported_modules: HashMap<NormalizedModulePath, Vec<Import>>,
        is_dts: bool,
    ) -> Module<'a> {
        Module {
            path,
            exports,
            imported_modules,
            is_wildcard_imported: Default::default(),
            is_dts,
        }
    }

    pub fn is_wildcard_imported(&self) -> bool {
        self.is_wildcard_imported.get()
    }

    pub fn mark_wildcard_imported(&self) {
        self.is_wildcard_imported.set(true)
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

pub fn resolve_import_path(
    project_root: &Path,
    current_folder: &Path,
    import_source: &str,
) -> anyhow::Result<NormalizedModulePath> {
    let mut absolute_path = RelativePath::new(import_source).to_logical_path(current_folder);

    for ext in ["d.ts", "ts", "tsx"] {
        let with_ext = absolute_path.clone().with_extension(ext);
        if with_ext.is_file() {
            return normalize_module_path(project_root, &with_ext);
        }
    }

    absolute_path.push("index.ts");
    normalize_module_path(project_root, &absolute_path)
}
