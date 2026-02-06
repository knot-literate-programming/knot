# Plan de Commits - Améliorations knot-core

**Date** : 6 février 2026
**Basé sur** : Analyse qualité du code (KNOT_CORE_QUALITY_ANALYSIS.md)

---

## 🎯 Stratégie de Commits

Les commits sont organisés en **3 phases** progressives :
- **Phase 1** : Quick wins (pas de breaking changes)
- **Phase 2** : Refactoring et robustesse
- **Phase 3** : Tests et documentation

Chaque commit est **atomique** et peut être mergé indépendamment.

---

## Phase 1 : Quick Wins (4 commits)

### Commit 1.1 : Nettoyer les attributs inutiles

**Type** : `chore(executors)`
**Message** :
```
chore(executors): remove unnecessary dead_code attribute

The cache_dir field in PythonExecutor is actually used in
save_constant() and load_constant() methods, so the #[allow(dead_code)]
attribute is not needed.
```

**Fichiers modifiés** :
- `crates/knot-core/src/executors/python/mod.rs`

**Changements** :
```diff
 pub struct PythonExecutor {
     process: PythonProcess,
-    #[allow(dead_code)]
     cache_dir: PathBuf,
 }
```

**Estimation** : 5 minutes

---

### Commit 1.2 : Uniformiser les boundary markers

**Type** : `refactor(executors)`
**Message** :
```
refactor(executors): unify boundary markers across executors

Use the same BOUNDARY_MARKER constant from defaults for consistency
across R and Python executors. This makes the codebase more maintainable
and reduces confusion.
```

**Fichiers modifiés** :
- `crates/knot-core/src/executors/python/process.rs`
- `crates/knot-core/src/executors/r/mod.rs`

**Changements** :
```diff
 // crates/knot-core/src/executors/python/process.rs
-const BOUNDARY: &str = "---KNOT_BOUNDARY---";
+const BOUNDARY: &str = crate::defaults::Defaults::BOUNDARY_MARKER;

 // crates/knot-core/src/executors/r/mod.rs
-const BOUNDARY: &str = crate::defaults::Defaults::BOUNDARY_MARKER;
+use crate::defaults::Defaults::BOUNDARY_MARKER as BOUNDARY;
```

**Estimation** : 10 minutes

---

### Commit 1.3 : Ajouter documentation de module manquante

**Type** : `docs(core)`
**Message** :
```
docs(core): add module-level documentation for key modules

Add module-level documentation (//! comments) for parser and compiler
modules to improve code comprehension and maintainability.

- parser/mod.rs: Explain AST structure and parsing approach
- parser/options.rs: Document chunk options parsing logic
- compiler/chunk_processor.rs: Explain chunk processing and caching
- compiler/inline_processor.rs: Document inline expression handling
```

**Fichiers modifiés** :
- `crates/knot-core/src/parser/mod.rs`
- `crates/knot-core/src/parser/options.rs`
- `crates/knot-core/src/compiler/chunk_processor.rs`
- `crates/knot-core/src/compiler/inline_processor.rs`

**Exemple** :
```rust
// crates/knot-core/src/parser/mod.rs
//! Knot document parsing
//!
//! This module provides parsing functionality for .knot files, which are
//! Typst documents with embedded R/Python code chunks.
//!
//! # Architecture
//!
//! - `ast.rs`: Abstract Syntax Tree definitions (Chunk, InlineExpr, Document)
//! - `options.rs`: Chunk option parsing (#| key: value syntax)
//! - `winnow_parser.rs`: Main parser implementation using winnow combinator library
//!
//! # Usage
//!
//! ```rust
//! use knot_core::parser::parse_document;
//!
//! let source = "...knot file content...";
//! let doc = parse_document(source)?;
//! ```

mod ast;
mod options;
mod winnow_parser;
```

**Estimation** : 30 minutes

---

### Commit 1.4 : Documenter thread-safety pour side_channel

**Type** : `docs(executors)`
**Message** :
```
docs(executors): document thread-safety requirements for side_channel

Clarify that SideChannel::setup_env() must be called in a single-threaded
context due to unsafe std::env::set_var usage. Add safety documentation.
```

**Fichiers modifiés** :
- `crates/knot-core/src/executors/side_channel.rs`

**Changements** :
```diff
+    /// Setup environment variable for side-channel communication
+    ///
+    /// # Safety
+    ///
+    /// This method uses `unsafe { std::env::set_var() }` which is not thread-safe.
+    /// It MUST be called in a single-threaded context (e.g., before spawning
+    /// executor threads). In the current architecture, this is guaranteed because
+    /// the compiler runs chunks sequentially in a single thread.
+    ///
+    /// # Panics
+    ///
+    /// May panic if called concurrently with other environment modifications.
     pub fn setup_env(&self) -> Result<()> {
         unsafe {
             std::env::set_var("KNOT_METADATA_FILE", &self.metadata_file);
         }
         Ok(())
     }
```

**Estimation** : 15 minutes

---

## Phase 2 : Refactoring et Robustesse (5 commits)

### Commit 2.1 : Extraire helper pour échappement de chemins

**Type** : `refactor(executors)`
**Message** :
```
refactor(executors): extract path escaping helper function

Create a reusable path_utils module to eliminate code duplication
across R and Python executors. The escape_path_for_code() helper
properly escapes Windows backslashes for use in code strings.

This reduces duplication from 27 instances to a single implementation.
```

**Fichiers créés** :
- `crates/knot-core/src/executors/path_utils.rs`

**Fichiers modifiés** :
- `crates/knot-core/src/executors/mod.rs`
- `crates/knot-core/src/executors/python/mod.rs`
- `crates/knot-core/src/executors/r/mod.rs`

**Nouveau fichier** :
```rust
// crates/knot-core/src/executors/path_utils.rs
//! Path utilities for code generation
//!
//! Helpers for safely embedding file paths in generated R/Python code.

use std::path::Path;

/// Escape a path for safe use in code strings
///
/// Converts Windows backslashes to double-backslashes to prevent
/// escape sequence issues when embedding paths in R/Python code.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use knot_core::executors::path_utils::escape_path_for_code;
///
/// let path = Path::new(r"C:\Users\data.csv");
/// let escaped = escape_path_for_code(path);
/// assert_eq!(escaped, r"C:\\Users\\data.csv");
/// ```
pub fn escape_path_for_code(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_escape_path_windows() {
        let path = Path::new(r"C:\Users\file.txt");
        assert_eq!(escape_path_for_code(path), r"C:\\Users\\file.txt");
    }

    #[test]
    fn test_escape_path_unix() {
        let path = Path::new("/home/user/file.txt");
        assert_eq!(escape_path_for_code(path), "/home/user/file.txt");
    }
}
```

**Modifications** :
```diff
 // crates/knot-core/src/executors/mod.rs
 pub mod manager;
+pub mod path_utils;
 pub mod python;
 pub mod r;

 // crates/knot-core/src/executors/python/mod.rs
+use super::path_utils::escape_path_for_code;

 // Dans save_constant():
-let path_str = path.to_string_lossy().replace('\\', "\\\\");
+let path_str = escape_path_for_code(&path);
```

**Estimation** : 45 minutes

---

### Commit 2.2 : Améliorer sécurité - échapper les quotes dans les chemins

**Type** : `fix(executors)`
**Message** :
```
fix(executors): properly escape quotes in file paths for code generation

Add quote escaping to prevent potential code injection if file paths
contain single or double quotes. This hardens path handling in code
string generation.
```

**Fichiers modifiés** :
- `crates/knot-core/src/executors/path_utils.rs`

**Changements** :
```diff
 pub fn escape_path_for_code(path: &Path) -> String {
-    path.to_string_lossy().replace('\\', "\\\\")
+    path.to_string_lossy()
+        .replace('\\', "\\\\")
+        .replace('\'', "\\'")
+        .replace('"', "\\\"")
 }
```

**Tests** :
```rust
#[test]
fn test_escape_path_with_quotes() {
    let path = Path::new(r"C:\User's\data.csv");
    assert_eq!(escape_path_for_code(path), r"C:\\User\'s\\data.csv");
}

#[test]
fn test_escape_path_with_double_quotes() {
    let path = Path::new(r#"C:\Users\"special"\file.txt"#);
    assert_eq!(escape_path_for_code(path), r#"C:\\Users\\\"special\"\\file.txt"#);
}
```

**Estimation** : 30 minutes

---

### Commit 2.3 : Protéger contre injection de code dans noms de variables

**Type** : `security(executors)`
**Message** :
```
security(executors): use environment variables for object names

Replace direct string interpolation of variable names with environment
variables to prevent potential code injection if variable names contain
special characters.

This affects:
- Object hashing (digest/xxhash calls)
- Object deletion from globals
- Constant object loading/saving
```

**Fichiers modifiés** :
- `crates/knot-core/src/executors/r/mod.rs`
- `crates/knot-core/src/executors/python/mod.rs`

**Changements R** :
```diff
 // Dans hash_object():
-let code = format!(
-    r#"digest::digest({}, algo = "xxhash64")"#,
-    object_name
-);
+// Use environment variable to avoid code injection
+std::env::set_var("KNOT_OBJECT_NAME", object_name);
+let code = r#"digest::digest(get(Sys.getenv("KNOT_OBJECT_NAME")), algo = "xxhash64")"#;
```

**Changements Python** :
```diff
 // Dans delete_object():
-let code = format!("del globals()['{}']", object_name);
+std::env::set_var("KNOT_OBJECT_NAME", object_name);
+let code = r#"del globals()[os.environ["KNOT_OBJECT_NAME"]]"#;
```

**Estimation** : 1 heure

---

### Commit 2.4 : Extraire snapshot management dans module séparé

**Type** : `refactor(compiler)`
**Message** :
```
refactor(compiler): extract snapshot management into separate module

Move complex snapshot restoration logic (188 lines) from compiler/mod.rs
into a dedicated snapshot_manager module for better organization and
testability.

This improves:
- Code organization and readability
- Testability of snapshot logic
- Separation of concerns
```

**Fichiers créés** :
- `crates/knot-core/src/compiler/snapshot_manager.rs`

**Fichiers modifiés** :
- `crates/knot-core/src/compiler/mod.rs`

**Nouveau module** :
```rust
// crates/knot-core/src/compiler/snapshot_manager.rs
//! Snapshot Management for Incremental Compilation
//!
//! Handles loading and restoring language runtime state (constant objects)
//! from cache to enable incremental compilation. When a document is
//! recompiled, we restore the state of previously executed chunks to
//! avoid re-executing unchanged code.
//!
//! # Architecture
//!
//! - Load metadata from cache
//! - Identify chunks that need restoration (cached but not yet executed)
//! - Restore constant objects for each chunk in order
//! - Update hash chain state
//!
//! # Hash Chaining
//!
//! Each chunk's hash depends on previous chunks' hashes, creating a chain.
//! When restoring, we must maintain this chain to properly detect changes.

use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Result;

use crate::cache::metadata::CacheMetadata;
use crate::executors::manager::ExecutorManager;

pub struct SnapshotManager {
    // ... structure and implementation moved from compiler/mod.rs
}

impl SnapshotManager {
    pub fn new() -> Self { ... }

    pub fn restore_from_cache(
        &mut self,
        metadata: &CacheMetadata,
        executor_manager: &mut ExecutorManager,
    ) -> Result<()> {
        // Logic moved from compiler/mod.rs lines 112-300
    }
}
```

**Estimation** : 1.5 heures

---

### Commit 2.5 : Ajouter validation de noms de variables

**Type** : `feat(parser)`
**Message** :
```
feat(parser): validate variable names in constant option

Add validation to ensure variable names in the 'constant' chunk option
are valid identifiers (no quotes, special chars, etc). This prevents
issues in code generation and improves error messages.
```

**Fichiers modifiés** :
- `crates/knot-core/src/parser/options.rs`

**Changements** :
```rust
// Ajouter fonction de validation
fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Must start with letter or underscore
    let first = name.chars().next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }

    // Rest must be alphanumeric or underscore
    name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

// Dans parse_options():
"constant" => {
    options.constant = value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|name| {
            if !is_valid_identifier(name) {
                errors.push(format!(
                    "Invalid variable name '{}' in constant option. \
                     Must be a valid identifier (letters, digits, underscore).",
                    name
                ));
                false
            } else {
                true
            }
        })
        .collect();
}
```

**Estimation** : 45 minutes

---

## Phase 3 : Tests et Documentation (6 commits)

### Commit 3.1 : Ajouter tests pour path_utils

**Type** : `test(executors)`
**Message** :
```
test(executors): add comprehensive tests for path_utils

Test path escaping with various edge cases:
- Windows paths with backslashes
- Unix paths
- Paths with quotes
- Paths with spaces
- Empty paths
```

**Fichiers modifiés** :
- `crates/knot-core/src/executors/path_utils.rs`

**Tests ajoutés** :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_escape_path_windows() {
        let path = Path::new(r"C:\Users\file.txt");
        assert_eq!(escape_path_for_code(path), r"C:\\Users\\file.txt");
    }

    #[test]
    fn test_escape_path_unix() {
        let path = Path::new("/home/user/file.txt");
        assert_eq!(escape_path_for_code(path), "/home/user/file.txt");
    }

    #[test]
    fn test_escape_path_with_single_quotes() {
        let path = Path::new(r"C:\User's\data.csv");
        assert_eq!(escape_path_for_code(path), r"C:\\User\'s\\data.csv");
    }

    #[test]
    fn test_escape_path_with_double_quotes() {
        let path = Path::new(r#"C:\Users\"special"\file.txt"#);
        assert_eq!(escape_path_for_code(path), r#"C:\\Users\\\"special\"\\file.txt"#);
    }

    #[test]
    fn test_escape_path_with_spaces() {
        let path = Path::new(r"C:\Program Files\app\data.csv");
        assert_eq!(escape_path_for_code(path), r"C:\\Program Files\\app\\data.csv");
    }

    #[test]
    fn test_escape_path_mixed() {
        let path = Path::new(r#"C:\User's\folder "test"\file.csv"#);
        let escaped = escape_path_for_code(path);
        assert!(escaped.contains(r"\'"));
        assert!(escaped.contains(r"\\"));
        assert!(escaped.contains(r#"\""#));
    }
}
```

**Estimation** : 30 minutes

---

### Commit 3.2 : Ajouter tests pour le parser (cas d'erreur)

**Type** : `test(parser)`
**Message** :
```
test(parser): add error case tests for chunk option parsing

Add tests for invalid chunk options:
- Invalid boolean values
- Invalid numeric values
- Invalid variable names in constant option
- Malformed option syntax
```

**Fichiers créés** :
- `crates/knot-core/src/parser/tests.rs`

**Fichiers modifiés** :
- `crates/knot-core/src/parser/mod.rs`

**Contenu** :
```rust
// crates/knot-core/src/parser/tests.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_invalid_boolean() {
        let options_block = r#"
#| eval: maybe
"#;
        let (opts, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("eval"));
        assert!(errors[0].contains("true") || errors[0].contains("false"));
    }

    #[test]
    fn test_parse_invalid_number() {
        let options_block = r#"
#| fig-width: not-a-number
"#;
        let (opts, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("fig-width"));
    }

    #[test]
    fn test_parse_invalid_variable_name() {
        let options_block = r#"
#| constant: valid_name, 123invalid, also-invalid
"#;
        let (opts, errors) = parse_options(options_block);
        assert_eq!(errors.len(), 2);
        assert!(opts.constant.contains(&"valid_name".to_string()));
        assert!(!opts.constant.contains(&"123invalid".to_string()));
    }

    #[test]
    fn test_parse_valid_options() {
        let options_block = r#"
#| eval: true
#| echo: false
#| fig-width: 7.0
#| constant: x, y_2, _private
"#;
        let (opts, errors) = parse_options(options_block);
        assert!(errors.is_empty());
        assert_eq!(opts.eval, Some(true));
        assert_eq!(opts.echo, Some(false));
        assert_eq!(opts.fig_width, Some(7.0));
        assert_eq!(opts.constant.len(), 3);
    }
}
```

**Estimation** : 1 heure

---

### Commit 3.3 : Ajouter tests pour snapshot_manager

**Type** : `test(compiler)`
**Message** :
```
test(compiler): add unit tests for snapshot restoration

Test snapshot manager functionality:
- Basic restoration from cache
- Hash chain maintenance
- Handling missing cache files
- Handling corrupted metadata
- Empty snapshot scenarios
```

**Fichiers modifiés** :
- `crates/knot-core/src/compiler/snapshot_manager.rs`

**Tests** :
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_snapshot_restoration_basic() {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().to_path_buf();

        // Setup: create fake metadata
        // Test: restore snapshot
        // Assert: correct objects restored
    }

    #[test]
    fn test_snapshot_empty_cache() {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().to_path_buf();

        let mut manager = SnapshotManager::new();
        let result = manager.restore_from_empty_cache();
        assert!(result.is_ok());
    }

    #[test]
    fn test_snapshot_hash_chain_maintained() {
        // Test that hash chain is properly maintained during restoration
    }

    #[test]
    fn test_snapshot_missing_cache_file() {
        // Test graceful handling when referenced cache file is missing
    }
}
```

**Estimation** : 1.5 heures

---

### Commit 3.4 : Ajouter tests de corruption du cache

**Type** : `test(cache)`
**Message** :
```
test(cache): add cache corruption and recovery tests

Test atomic write behavior and recovery scenarios:
- Corrupted metadata.json
- Missing cache files
- Partial writes
- Concurrent access attempts
```

**Fichiers créés** :
- `crates/knot-core/src/cache/corruption_tests.rs`

**Fichiers modifiés** :
- `crates/knot-core/src/cache/mod.rs`

**Tests** :
```rust
#[cfg(test)]
mod corruption_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_corrupted_metadata_json() {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path();

        // Write invalid JSON
        fs::write(cache_dir.join("metadata.json"), "{ invalid json").unwrap();

        // Should return default metadata, not crash
        let metadata = storage::load_metadata(cache_dir);
        assert_eq!(metadata.chunks.len(), 0);
    }

    #[test]
    fn test_missing_cache_file() {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path();

        // Create metadata referencing non-existent file
        let mut metadata = CacheMetadata::default();
        metadata.chunks.push(ChunkCacheEntry {
            hash: "abc123".to_string(),
            files: vec!["nonexistent.txt".to_string()],
            // ...
        });

        storage::save_metadata(cache_dir, &metadata).unwrap();

        // Attempting to load should error
        let result = storage::get_cached_result(cache_dir, "abc123", &metadata);
        assert!(result.is_err());
    }

    #[test]
    fn test_atomic_write_safety() {
        // Test that interrupted writes don't corrupt cache
        // This is hard to test directly, but we can verify
        // that temp files are in same directory
    }
}
```

**Estimation** : 1 heure

---

### Commit 3.5 : Documenter winnow_parser en détail

**Type** : `docs(parser)`
**Message** :
```
docs(parser): add comprehensive documentation for winnow_parser

Add detailed module and function documentation explaining:
- Grammar rules and parsing strategy
- Winnow combinator usage patterns
- Error handling approach
- Examples of valid/invalid syntax
```

**Fichiers modifiés** :
- `crates/knot-core/src/parser/winnow_parser.rs`

**Documentation** :
```rust
//! Winnow-based Parser for Knot Documents
//!
//! This module implements the main parser for .knot files using the winnow
//! parser combinator library. Knot files are Typst documents with embedded
//! R/Python code chunks and inline expressions.
//!
//! # Grammar
//!
//! A knot document consists of:
//! - **Code chunks**: Blocks starting with triple backticks and language identifier
//! - **Inline expressions**: Expressions wrapped in backticks with language prefix
//! - **Typst content**: Everything else (passed through unchanged)
//!
//! ## Code Chunk Syntax
//!
//! ```knot
//! ```{r}
//! #| eval: true
//! #| echo: false
//!
//! x <- 1 + 1
//! print(x)
//! ```
//! ```
//!
//! ## Inline Expression Syntax
//!
//! ```knot
//! The result is `r x * 2`.
//! ```
//!
//! # Parser Strategy
//!
//! The parser uses a multi-pass approach:
//! 1. Scan for chunk/inline expression boundaries
//! 2. Extract and parse chunk options
//! 3. Build AST with position information
//! 4. Preserve Typst content verbatim
//!
//! # Error Handling
//!
//! Parsing errors are collected and returned with position information
//! to enable helpful error messages in the LSP and CLI.

// ... existing code with added doc comments on functions
```

**Estimation** : 2 heures

---

### Commit 3.6 : Optimiser metadata cache (optionnel)

**Type** : `perf(cache)`
**Message** :
```
perf(cache): add in-memory caching for metadata

Keep metadata in memory during compilation to avoid repeated
JSON serialization/deserialization. Only write to disk when
chunks are added or compilation completes.

This improves performance for large documents with many chunks.
```

**Fichiers modifiés** :
- `crates/knot-core/src/compiler/mod.rs`
- `crates/knot-core/src/cache/storage.rs`

**Changements** :
```rust
// Compiler keeps metadata in memory
pub struct Compiler {
    cache: Cache,
    metadata: CacheMetadata,  // <-- In-memory cache
    metadata_dirty: bool,     // <-- Track if needs saving
    // ...
}

// Only write when dirty
fn save_metadata_if_dirty(&mut self) -> Result<()> {
    if self.metadata_dirty {
        storage::save_metadata(&self.cache_dir, &self.metadata)?;
        self.metadata_dirty = false;
    }
    Ok(())
}
```

**Estimation** : 1 heure

---

## 📊 Résumé des Commits

| Phase | Commits | Estimation Totale |
|-------|---------|-------------------|
| **Phase 1 - Quick Wins** | 4 commits | **1h** |
| **Phase 2 - Refactoring** | 5 commits | **5h** |
| **Phase 3 - Tests & Docs** | 6 commits | **7h** |
| **TOTAL** | **15 commits** | **13h** |

---

## 🎯 Ordre d'Exécution Recommandé

### Jour 1 : Phase 1 (Quick Wins)
1. ✅ Commit 1.1 - Nettoyer attributs inutiles
2. ✅ Commit 1.2 - Uniformiser boundary markers
3. ✅ Commit 1.3 - Documentation modules
4. ✅ Commit 1.4 - Doc thread-safety

**Point de contrôle** : Tout compile, tous les tests passent

---

### Jour 2 : Phase 2 - Partie 1 (Refactoring)
5. ✅ Commit 2.1 - Extraire path helper
6. ✅ Commit 2.2 - Échapper quotes
7. ✅ Commit 2.5 - Validation variable names

**Point de contrôle** : Tests passent, pas de régression

---

### Jour 3 : Phase 2 - Partie 2 (Sécurité)
8. ✅ Commit 2.3 - Protection injection code
9. ⚠️ Commit 2.4 - Extraire snapshot manager (complexe)

**Point de contrôle** : Architecture améliorée, tests passent

---

### Jour 4 : Phase 3 - Tests
10. ✅ Commit 3.1 - Tests path_utils
11. ✅ Commit 3.2 - Tests parser erreurs
12. ✅ Commit 3.3 - Tests snapshot_manager
13. ✅ Commit 3.4 - Tests corruption cache

**Point de contrôle** : Couverture de tests améliorée

---

### Jour 5 : Phase 3 - Documentation & Polish
14. ✅ Commit 3.5 - Doc winnow_parser
15. 🟡 Commit 3.6 - Optimisation metadata (optionnel)

**Point de contrôle** : Code documenté, prêt pour release

---

## 🚦 Critères de Validation

Pour chaque commit :

### ✅ Avant le commit
- [ ] Code compile sans warnings
- [ ] Tests existants passent (`cargo test`)
- [ ] Nouveaux tests ajoutés si pertinent
- [ ] Documentation mise à jour
- [ ] `cargo clippy` sans warnings
- [ ] `cargo fmt` appliqué

### ✅ Message de commit
- [ ] Type correct (feat/fix/docs/test/refactor/perf/chore)
- [ ] Scope indiqué (executors/parser/cache/compiler)
- [ ] Description claire et concise
- [ ] Body explique le "pourquoi" si nécessaire

### ✅ Après le commit
- [ ] Commit atomique (change une seule chose)
- [ ] Peut être mergé indépendamment
- [ ] Pas de breaking changes (sauf si documenté)

---

## 🔄 Workflow Git Recommandé

```bash
# Créer une branche pour chaque phase
git checkout -b improve/phase-1-quick-wins
# ... faire commits 1.1 à 1.4
git push origin improve/phase-1-quick-wins
# Créer PR, review, merge

git checkout -b improve/phase-2-refactoring
# ... faire commits 2.1 à 2.5
git push origin improve/phase-2-refactoring
# Créer PR, review, merge

git checkout -b improve/phase-3-tests-docs
# ... faire commits 3.1 à 3.6
git push origin improve/phase-3-tests-docs
# Créer PR, review, merge
```

---

## 📝 Notes Importantes

### Commits Complexes

**Commit 2.4** (Extraire snapshot_manager) :
- Le plus complexe et risqué
- Tester extensivement avant de merger
- Peut être décomposé en 2 commits :
  1. Créer module avec code copié
  2. Supprimer code de compiler/mod.rs

**Commit 3.5** (Doc winnow_parser) :
- Demande compréhension profonde du parser
- Peut prendre plus de temps que prévu
- Peut être fait en plusieurs sessions

### Commits Optionnels

**Commit 3.6** (Optimisation metadata) :
- Gain de performance marginal
- Peut être reporté si manque de temps
- Ajoute de la complexité (gestion de dirty flag)

---

## ✨ Résultat Final Attendu

Après tous les commits :

### 📈 Amélioration du Score
- **Avant** : 7.6/10
- **Après Phase 1** : 7.8/10
- **Après Phase 2** : 8.2/10
- **Après Phase 3** : 8.5/10

### 🎯 Bénéfices
- ✅ Code plus maintenable (moins de duplication)
- ✅ Meilleure sécurité (protection injection)
- ✅ Tests plus complets (confiance accrue)
- ✅ Documentation complète (onboarding facile)
- ✅ Architecture plus claire (séparation concerns)

---

*Plan créé le 6 février 2026*
