# CLAUDE.md - Conventions et Workflow

Ce fichier contient les conventions de développement et pointeurs rapides pour Claude.

## 🔴 À lire en début de session

**IMPORTANT**: Avant de commencer à coder, lire dans cet ordre:
1. **`CLAUDE.md`** (ce fichier) - conventions et workflow
2. **`DEVLOG.md`** - historique chronologique des sessions et implémentations
3. **`knot-project-reference.txt`** - architecture, phases, et plans du projet

Ces trois fichiers donnent le contexte complet pour reprendre le développement.

## Workflow de développement

### Tests et validation
- **Toujours** créer des exemples dans `examples/` plutôt que des tests isolés
- Exécuter `cargo test` avant chaque commit
- Les tests doivent couvrir le parsing ET la résolution des options

### Commits
- Format des commits avec co-auteur:
  ```
  git commit -m "$(cat <<'EOF'
  type(scope): description

  Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
  EOF
  )"
  ```
- Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

### Création de fichiers
- **NE PAS** créer de fichiers markdown ou documentation sans demande explicite
- **TOUJOURS** préférer éditer un fichier existant plutôt que créer un nouveau
- Les nouveaux modules doivent être exportés dans `lib.rs`

## Structure clé du projet

### Core (`crates/knot-core/src/`)
- **`parser.rs`** : Parsing des chunks et options (`ChunkOptions`, `Document::parse()`)
- **`executors/mod.rs`** : Enum `ExecutionResult` et trait `Executor`
- **`executors/r.rs`** : Exécuteur R avec gestion du cache et package knot
- **`compiler.rs`** : Génération du `.typ` final depuis le `.knot`
- **`cache.rs`** : Système de cache SHA256-based avec invalidation
- **`graphics.rs`** : Options graphiques (defaults, config, résolution)

### CLI (`crates/knot-cli/src/`)
- **`main.rs`** : Point d'entrée, commandes `compile`, `init`, `clean`
- Fonction `fix_paths_in_typst()` : Copie CSVs vers `_knot_files/`

### Package R (`knot-r-package/`)
- **`R/typst.R`** : Méthodes S3 pour conversion (DataFrames → CSV, Plots → SVG/PNG)
- **`NAMESPACE`** : Exports S3method requis

### Package Typst (`knot-typst-package/`)
- **`lib.typ`** : Fonction `#code-chunk()` pour affichage
- Nécessite `#show: codly-init` dans les documents

## Conventions de code

### Options de chunks
- Stockées dans `ChunkOptions` (parser.rs)
- Nouvelles options: ajouter au struct + parser + tests
- Nommage: kebab-case dans `.knot` → snake_case en Rust (`fig-width` → `fig_width`)

### Hiérarchie des options
Priorité (plus élevée en premier):
1. Options au niveau chunk (`#| fig-width: 10`)
2. Config document YAML frontmatter (futur)
3. Defaults hardcodés (`GraphicsDefaults::default()`)

### Gestion des fichiers générés
- Cache: `.knot_cache/` avec noms basés sur SHA256
- Fichiers de sortie: `_knot_files/` (copie depuis cache)
- Pattern knitr-style pour compatibilité

## Patterns de résolution

### Graphics options
```rust
let defaults = GraphicsDefaults::default();
let doc_graphics = None; // ou Some(GraphicsConfig { ... })
let resolved = resolve_graphics_options(&chunk.options, &doc_graphics, &defaults);
```

### Exécution et cache
```rust
// Check cache d'abord
if let Some(cached) = cache_manager.get_cached_result(&chunk) {
    return Ok(cached);
}

// Sinon exécuter et cacher
let result = executor.execute_chunk(&chunk)?;
cache_manager.cache_result(&chunk, &result)?;
```

## Phases du projet

Voir `knot-project-reference.txt` pour détails complets.

- **Phase 1** : ✅ Exécution R de base
- **Phase 2** : ✅ Package R et DataFrames → Typst tables
- **Phase 3** : ✅ Système de cache
- **Phase 4** : 🚧 Graphics (4A: bitmap/vectoriel, 4B: natif Typst)
  - Infrastructure options: ✅ Parsing et résolution
  - Génération: ❌ À implémenter

## Rappels importants

- Le regex de parsing des chunks est dans `crates/knot-core/src/lib.rs` (`CHUNK_REGEX`)
- Marker CSV dans stdout R: `__KNOT_SERIALIZED_CSV__`
- Syntaxe table Typst: `#table(columns: data.first().len(), ..csv("path").flatten())`
- Template par défaut: `templates/default.knot`
