//! CLI formatting: prepare every source before writing any changes.
use anyhow::{Context, Result};
use knot_core::{CodeFormatter, Config, Document, ProjectPaths};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// Format an explicit file, or the main source and declared includes of a project.
/// Returns changed paths. Check mode never writes; formatter/read failures occur
/// before any write. A filesystem write failure can still leave earlier writes applied.
pub fn format_sources(file: Option<&Path>, start: &Path, check: bool) -> Result<Vec<PathBuf>> {
    let files = if let Some(file) = file {
        vec![file.to_path_buf()]
    } else {
        project_sources(start)?
    };
    format_paths(&files, check)
}

/// Format one file, returning whether it differs (also in check mode).
pub fn format_file(file: &Path, check: bool) -> Result<bool> {
    Ok(!format_paths(&[file.to_path_buf()], check)?.is_empty())
}

fn project_sources(start: &Path) -> Result<Vec<PathBuf>> {
    let (config, root) = Config::find_and_load(start)?;
    anyhow::ensure!(
        root.join("knot.toml").is_file(),
        "Could not find knot.toml; specify a file or run inside a Knot project"
    );
    let main = ProjectPaths::resolve(&config, &root)?.main_file;
    let root = root.canonicalize()?;
    let includes = config.document.includes.unwrap_or_default();
    let candidates = std::iter::once(main).chain(includes.iter().map(|name| root.join(name)));
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for path in candidates {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("Cannot resolve source {}", path.display()))?;
        anyhow::ensure!(
            canonical.starts_with(&root),
            "Source {} is outside the project root",
            path.display()
        );
        if seen.insert(canonical.clone()) {
            files.push(canonical);
        }
    }
    Ok(files)
}

fn format_paths(files: &[PathBuf], check: bool) -> Result<Vec<PathBuf>> {
    let formatter = CodeFormatter::new(None, None);
    let mut changes = Vec::new();
    for file in files {
        let original =
            fs::read_to_string(file).with_context(|| format!("Cannot read {}", file.display()))?;
        let doc = Document::parse(original);
        if let Some(error) = doc.errors.first() {
            anyhow::bail!("Cannot format {}: {error}", file.display());
        }
        for chunk in &doc.chunks {
            if let Some(error) = chunk.errors.first() {
                anyhow::bail!(
                    "Cannot format {}:{}: {}",
                    file.display(),
                    chunk.range.start.line + 1,
                    error.message
                );
            }
        }
        // Format fallibly before reconstruction: the editor's optional callback
        // may retain unformatted code, but the CLI must report these failures.
        let mut chunks = Vec::with_capacity(doc.chunks.len());
        for chunk in &doc.chunks {
            chunks.push(
                formatter
                    .format_code(&chunk.code, &chunk.language)
                    .with_context(|| {
                        format!(
                            "Cannot format {}:{} ({} block)",
                            file.display(),
                            chunk.range.start.line + 1,
                            chunk.language
                        )
                    })?,
            );
        }
        let formatted = doc.format(|index, _, _| Some(std::mem::take(&mut chunks[index])));
        if formatted != doc.source {
            changes.push((file.clone(), formatted));
        }
    }
    if !check {
        for (file, text) in &changes {
            fs::write(file, text).with_context(|| format!("Cannot write {}", file.display()))?;
        }
    }
    Ok(changes.into_iter().map(|(file, _)| file).collect())
}
