# KNOT - PROJECT REFERENCE

> "knot is not knitr"

This document serves as the comprehensive reference for the Knot project, consolidating the original specification, architectural decisions, LSP design, and the development log.

---

# PART 1: SPECIFICATION & ROADMAP

(Originally `knot-project-reference.txt`)

================================================================================
KNOT - PROJET DE LITERATE PROGRAMMING POUR TYPST
Résumé structuré et guide de développement
"knot is not knitr"
================================================================================

TABLE DES MATIÈRES
================================================================================
1. Vue d'ensemble
2. Décisions de design fondamentales
3. Architecture technique
4. Configuration projet
5. Commandes CLI
6. Roadmap détaillée (Phases 1-9)
7. Système de cache avec chaînage (Phase 3)
8. Patterns de code importants
9. Exemples de documents
10. Critères de succès
11. Points d'attention
12. Différenciation vs alternatives
13. Estimation temps
14. Prochaine étape immédiate

================================================================================
1. VUE D'ENSEMBLE
================================================================================

KNOT est un système de literate programming moderne pour Typst, permettant 
d'exécuter du code R, LilyPond et Python directement dans des documents Typst.

NOM ET SLOGAN :
- Nom : knot (4 lettres, mémorable)
- Slogan : "knot is not knitr" (jeu de mots récursif comme GNU)
- Évocation : "knot" = nœud, lien, tisser (weave)

PHILOSOPHIE :
- Typst comme SEUL langage de documentation (pas de Markdown)
- Exécution STRICTEMENT LINÉAIRE et déterministe
- PERFORMANCE MAXIMALE (Rust + Typst vs Pandoc + LaTeX)
- REPRODUCTIBILITÉ GARANTIE
- Cache intelligent avec invalidation en cascade

POSITIONNEMENT :
"knot is not knitr" - Alternative moderne pour académiques voulant 
qualité typographique + performance + reproductibilité

================================================================================
2. DÉCISIONS DE DESIGN FONDAMENTALES
================================================================================

2.1 NOMENCLATURE
--------------------------------------------------------------------------------
Nom projet       : knot (à vérifier disponibilité sur crates.io)
Extension        : .knot (Knot document)
Packages Rust    : knot, knot-core, knot-cli
Package R        : knot
Package Typst    : @preview/knot
Cache directory  : .knot_cache/
Config file      : knot.toml

2.2 SYNTAXE DES CHUNKS
--------------------------------------------------------------------------------

CHUNKS EXÉCUTABLES (notation avec accolades) :

```{r}
#| eval: true
#| echo: true
#| output: true
x <- 1:10
mean(x)
```

CHUNKS AVEC NOM (optionnel) :

```{r chunk-name}
#| eval: true
data <- read_csv("data.csv")
```

CODE STATIQUE (exemples non exécutés) :

```r
# Juste un exemple, pas exécuté
example_function()
```

RATIONALE :
- ```{r}  = exécutable (comme R Markdown/Quarto)
- ```r    = statique (exemple de code)
- Distinction visuelle immédiate
- Compatibilité conceptuelle avec écosystème R

2.3 OPTIONS DE CHUNKS
--------------------------------------------------------------------------------

Syntaxe : commentaires #| (comme Quarto)

OPTIONS COMMUNES (tous langages) :
  eval: true/false       - Exécuter le code
  echo: true/false       - Afficher le code source
  output: true/false     - Afficher le résultat
  cache: true/false      - Utiliser le cache (défaut: true)
  label: <id>            - Pour références croisées
  caption: "..."         - Légende figure/tableau
  depends: [files...]    - Dépendances fichiers externes

OPTIONS SPÉCIFIQUES R :
  warning: true/false    - Afficher warnings R
  error: true/false      - Afficher erreurs ou stopper
  fig-width: <n>         - Largeur plot (pouces)
  fig-height: <n>        - Hauteur plot (pouces)
  fig-format: svg/png    - Format sortie
  tbl-cap: "..."         - Caption tableaux
  tbl-digits: <n>        - Arrondis tableaux

OPTIONS SPÉCIFIQUES LILYPOND :
  width: <measure>       - Largeur partition (160mm, 6in, etc.)
  staff-size: <n>        - Taille portée (12-26)
  format: svg/png        - Format sortie
  dpi: <n>               - Résolution si PNG

EXEMPLE AVEC DÉPENDANCES :

```{r load-data}
#| eval: true
#| depends: data/input.csv, scripts/utils.R
data <- read_csv("data/input.csv")
source("scripts/utils.R")
```

Si data/input.csv ou scripts/utils.R change → chunk invalidé et ré-exécuté

2.4 LANGAGES SUPPORTÉS (PRIORITÉS)
--------------------------------------------------------------------------------

Phase     Langage      Méthode             Priorité
--------  -----------  ------------------  ---------
v0.1-1.0  R            subprocess → extendr  ⭐⭐⭐
v2.0      LilyPond     subprocess            ⭐⭐
v2.5+     Python       PyO3                  ⭐

ISOLATION DES SESSIONS :
- Chaque langage a sa propre session
- PAS de partage de variables entre langages
- Communication via fichiers si nécessaire
- Continuité intra-langage garantie

Exemple :
  ```{r}
  x <- 42
  ```
  
  ```{python}
  # x n'existe PAS ici (session Python différente)
  y = [1, 2, 3]
  ```
  
  ```{r}
  # x existe toujours (même session R)
  print(x)  # 42
  ```

================================================================================
3. ARCHITECTURE TECHNIQUE
================================================================================

3.1 STRUCTURE DU PROJET
--------------------------------------------------------------------------------

knot/
├── Cargo.toml                    # Workspace
├── README.md
├── LICENSE
├── crates/
│   ├── knot-core/                # Bibliothèque (logique métier)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── parser.rs         # Parse .knot, extrait chunks
│   │       ├── cache.rs          # Système de cache chaîné
│   │       ├── compiler.rs       # Orchestration compilation
│   │       ├── codegen.rs        # Génération Typst
│   │       └── executors/
│   │           ├── mod.rs        # Trait LanguageExecutor
│   │           ├── r.rs          # Executor R
│   │           ├── lilypond.rs   # Executor LilyPond (futur)
│   │           └── python.rs     # Executor Python (futur)
│   ├── knot-cli/                 # CLI wrapper (~200 lignes)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs
│   └── knot-lsp/                 # LSP (v2.0+, stub)
│       └── src/
│           └── main.rs
├── r-package/                    # Package R (Phase 2+)
│   ├── DESCRIPTION
│   ├── NAMESPACE
│   └── R/
│       ├── typst-output.R        # Générique + méthodes
│       ├── tables.R              # data.frame → Typst
│       └── plots.R               # ggplot2 support
├── typst-package/                # Package Typst (Phase 5+)
│   ├── typst.toml
│   └── lib.typ                   # Fonctions #rtable(), etc.
├── examples/
│   └── demo.knot
└── tests/
    └── integration/

3.2 DÉPENDANCES RUST
--------------------------------------------------------------------------------

[workspace.dependencies]
knot-core = { path = "crates/knot-core" }
clap = { version = "4.5", features = ["derive"] }
extendr-api = "0.7"                # Pour R (Phase 2+)
regex = "1.10"
sha2 = "0.10"                      # Pour cache
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
tempfile = "3.8"
chrono = "0.4"                     # Pour timestamps cache

3.3 TRAIT LANGUAGEEXECUTOR
--------------------------------------------------------------------------------

pub trait LanguageExecutor {
    fn initialize(&mut self) -> Result<()>;
    fn execute(&mut self, code: &str) -> Result<ExecutionResult>;
}

pub enum ExecutionResult {
    Text(String),
    Plot(PathBuf),
    Both { text: String, plot: PathBuf },
}

Ce trait permet d'ajouter facilement de nouveaux langages sans modifier
l'architecture globale.

3.4 WORKFLOW DE COMPILATION
--------------------------------------------------------------------------------

document.knot
    ↓
  PARSE (regex pour Phase 1)
    ↓
Chunks extraits (Vec<Chunk>)
    ↓
  CALCUL HASH CHAÎNÉ pour chaque chunk
  Hash_n = sha256(Code_n + Options_n + Hash_{n-1} + hash(dépendances))
    ↓
  CHECK CACHE
    ↓
  Si cache hit: charger résultat
  Si cache miss: EXÉCUTION (RExecutor, etc.)
    ↓
Résultats (Vec<ExecutionResult>)
    ↓
  GÉNÉRATION TYPST (injection dans document)
    ↓
document.typ (intermédiaire)
    ↓
  TYPST COMPILE
    ↓
document.pdf

3.5 STRUCTURES DE DONNÉES PRINCIPALES
--------------------------------------------------------------------------------

// Position dans le fichier (ligne/colonne, base 0)
// Essentiel pour le support LSP (Language Server Protocol)
pub struct Position {
    pub line: usize,
    pub column: usize,
}

// Plage dans le fichier, de `start` (inclusif) à `end` (exclusif)
pub struct Range {
    pub start: Position,
    pub end: Position,
}

pub struct Chunk {
    pub language: String,
    pub name: Option<String>,      // Nom optionnel du chunk
    pub code: String,
    pub options: ChunkOptions,
    pub range: Range,       // Position du chunk entier (de ```{r} à ```)
    pub code_range: Range,  // Position du code seul à l'intérieur
}

pub struct ChunkOptions {
    pub eval: bool,
    pub echo: bool,
    pub output: bool,
    pub cache: bool,
    pub label: Option<String>,
    pub caption: Option<String>,
    pub depends: Vec<PathBuf>,     // ⭐ Dépendances fichiers
    // + options spécifiques par langage
}

pub struct Document {
    pub source: String,
    pub chunks: Vec<Chunk>,
}

3.6 SYSTÈME DE CACHE AVEC CHAÎNAGE
--------------------------------------------------------------------------------

PRINCIPE : Hash_n = sha256(Code_n + Options_n + Hash_{n-1} + hash(dépendances))

AVANTAGES :
1. Invalidation en cascade automatique
2. Dépendances implicites capturées
3. Dépendances explicites (fichiers) gérées
4. Déterministe et reproductible

EXEMPLE :

Chunk 1: x <- read_csv("data.csv")
  Hash_1 = sha256(code_1 + options_1 + "" + sha256(data.csv))
  
Chunk 2: y <- mean(x)
  Hash_2 = sha256(code_2 + options_2 + Hash_1)
  
Chunk 3: plot(x, y)
  Hash_3 = sha256(code_3 + options_3 + Hash_2)

Si data.csv change → Hash_1 invalide → Hash_2 invalide → Hash_3 invalide
Si Chunk 2 change → Hash_2 invalide → Hash_3 invalide
Si Chunk 1 inchangé → cache hit sur tous les chunks

STRUCTURE CACHE :

.knot_cache/
├── metadata.json                # Index complet
│   {
│     "document_hash": "abc...",
│     "chunks": [
│       {
│         "index": 0,
│         "name": "setup",
│         "hash": "def...",
│         "files": ["chunk_def.txt"],
│         "dependencies": ["data/input.csv"],
│         "updated_at": "2024-01-07T10:00:00Z"
│       }
│     ]
│   }
├── chunk_def123.txt             # Sortie texte chunk 0
├── chunk_ghi456.svg             # Plot chunk 1
└── chunk_jkl789.txt             # Sortie texte chunk 2

================================================================================
4. CONFIGURATION PROJET
================================================================================

4.1 KNOT.TOML (OPTIONNEL)
--------------------------------------------------------------------------------

[project]
name = "mon-projet"

[document]
main = "main.knot"
output = "output/document.pdf"
text_width = "160mm"              # Pour LilyPond

[cache]
enabled = true
directory = ".knot_cache"
# Fichiers à surveiller (invalide tout le cache si changé)
watch = ["data/*.csv", "scripts/*.R"]

[languages]
r = true
lilypond = false
python = false

[languages.r]
auto_load = ["tidyverse"]
use_renv = false

[languages.r.defaults]
eval = true
echo = true
fig_format = "svg"
fig_width = 6
fig_height = 4

4.2 DÉTECTION AUTOMATIQUE PROJET
--------------------------------------------------------------------------------

Le CLI remonte les dossiers pour trouver knot.toml (comme Cargo/Git).

Exemple :
  mon-cours/
  ├── knot.toml
  └── chapters/
      └── chapter1.knot

  $ cd mon-cours/chapters/
  $ knot compile chapter1.knot
  # ✓ Trouve knot.toml dans ../
  # ✓ Utilise config du projet

4.3 FICHIER .GITIGNORE RECOMMANDÉ
--------------------------------------------------------------------------------

# Knot cache
.knot_cache/

# Outputs
output/
*.pdf

# Fichiers intermédiaires (optionnel)
# *.typ

# R
.Rhistory
.RData
.Rproj.user/
renv/library/

# Python
.venv/
__pycache__/

# OS
.DS_Store
Thumbs.db

================================================================================
5. COMMANDES CLI
================================================================================

# Initialiser document simple
knot init article.knot

# Initialiser projet structuré
knot init mon-cours --project

# Compiler
knot compile document.knot

# Compiler avec options
knot compile document.knot --output mon-fichier.pdf

# Compiler sans cache (force ré-exécution)
knot compile document.knot --no-cache
knot compile document.knot --force

# Mode watch (futur Phase 6)
knot watch document.knot

# Check sans compiler (futur)
knot check document.knot

# Clean cache
knot clean                        # Supprime tout .knot_cache/
knot clean --keep-metadata        # Garde metadata.json pour inspection

# Infos cache (futur)
knot cache info                   # Affiche stats cache
knot cache list                   # Liste chunks cachés

================================================================================
6. ROADMAP DÉTAILLÉE
================================================================================

6.1 PHASE 1 : PROTOTYPE MINIMAL (Semaines 1-4) ⭐ PRIORITAIRE
--------------------------------------------------------------------------------

OBJECTIF : Valider le concept avec document simple

DELIVERABLE : Compiler avec succès un document avec 2-3 chunks R simples

--- SEMAINE 1 : Setup & Parser ---

Jour 1-2 : Initialisation projet
  • Créer workspace Cargo
  • Initialiser crates/knot-core et crates/knot-cli
  • Ajouter dépendances de base (clap, regex, anyhow, sha2)
  • Vérifier compilation

Jour 3-4 : Parser minimal (regex-based)
  • Fichier parser.rs
  • Regex pour détecter ```{r} avec options #|
  • Structures Chunk, ChunkOptions, Document
  • Support option depends (Vec<PathBuf>)
  • Tests unitaires parser

  REGEX CLEF :
  r"(?s)```\{(r|python|lilypond)\s*([^\}]*)\}
((?://\|.*\n)*)(.*?)```"
  
  Groupes :
    1: language (r, python, lilypond)
    2: chunk name (optionnel)
    3: options block (#| lines)
    4: code

Jour 5 : CLI minimal
  • Fichier main.rs
  • Sous-commandes : init, compile
  • knot init crée template
  • knot compile parse et affiche info chunks
  • Pas d'exécution encore

  TEST : knot init test.knot && knot compile test.knot
  RÉSULTAT ATTENDU : "Found 1 chunks, Chunk 0: r (eval=true)"

CHECKPOINT SEMAINE 1 : ✅ Parser fonctionne, CLI basique opérationnel

--- SEMAINE 2 : Exécution R basique ---

Jour 1-2 : RExecutor via subprocess
  • Fichier executors/r.rs
  • Implémenter trait LanguageExecutor
  • Méthode execute() : créer fichier temp, lancer R, capturer stdout
  • Gestion erreurs basique

  IMPLÉMENTATION SIMPLIFIÉE (subprocess) :
    - Créer NamedTempFile avec code
    - Exécuter : R --vanilla --quiet < temp.R
    - Capturer stdout/stderr
    - Retourner ExecutionResult::Text(output)

Jour 3-4 : Intégration executor dans compiler
  • Fichier compiler.rs
  • Struct Compiler avec r_executor
  • Méthode compile(doc) : itérer chunks, exécuter si eval=true
  • Construire résultats (vec de strings Typst)
  • PAS DE CACHE ENCORE (Phase 3)

Jour 5 : Génération Typst et compilation PDF
  • Fichier codegen.rs (basique)
  • Format code blocks : ```r\n{code}\n```
  • Format output blocks : #block(fill: luma(250))[```\n{output}\n```]
  • Écrire .typ, appeler typst compile

  TEST : Compiler test.knot → test.pdf
  VÉRIFIER : PDF contient code et résultats

CHECKPOINT SEMAINE 2 : ✅ Exécution R basique, génération PDF

--- SEMAINE 3 : Génération Typst intelligente ---

Jour 1-2 : Injection intelligente des chunks
  • Problème actuel : append résultats à la fin
  • Solution : remplacer chaque chunk par son résultat compilé
  • Améliorer codegen pour injection in-place

Jour 3-4 : Gestion erreurs R
  • Parser stderr R pour meilleurs messages
  • Afficher code qui a causé erreur
  • Context avec anyhow

Jour 5 : Test end-to-end complet
  • Créer test-full.knot avec multiples chunks
  • Tester différentes combinaisons eval/echo/output
  • Vérifier PDF final

CHECKPOINT SEMAINE 3 : ✅ Génération Typst intelligente, gestion erreurs

--- SEMAINE 4 : Polish et documentation ---

Jour 1-2 : Amélioration knot init
  • Template plus complet et professionnel
  • Headers, métadonnées
  • Exemple chunks variés

Jour 3 : README et documentation
  • README.md complet avec slogan "knot is not knitr"
  • Section Quick Start
  • Documentation syntaxe chunks
  • Exemples

Jour 4 : Tests unitaires
  • Tests parser (chunks multiples, options, edge cases)
  • Tests compilation (mock executor si besoin)
  • cargo test doit passer

Jour 5 : Finalisation prototype
  • Exemple demo.knot démonstration
  • Vérifier checklist Phase 1
  • Compiler demo.knot et inspecter PDF

CHECKPOINT SEMAINE 4 : ✅ Phase 1 complète et démo fonctionnelle

CRITÈRES DE SUCCÈS PHASE 1 :
  ✅ knot compile demo.knot produit PDF correct
  ✅ Code R exécuté, résultats visibles
  ✅ Options eval, echo, output fonctionnent
  ✅ Erreurs R capturées et affichées proprement
  ✅ cargo test passe tous les tests

LIMITATIONS ACCEPTABLES PHASE 1 :
  • Subprocess R (pas extendr embedded)
  • PAS DE CACHE (implémenté en Phase 3)
  • Pas de graphiques
  • Pas d'inline R (#r-inline[])
  • Génération Typst basique (pas de #rtable(), etc.)

6.2 PHASE 2 : PACKAGE R (Semaines 5-8)
--------------------------------------------------------------------------------

OBJECTIF : Conversion propre R → Typst

IMPLÉMENTATION :
  • Structure package R knot
  • Générique typst_output()
  • Méthodes pour data.frame, tibble, default
  • Helpers génération code Typst
  • Chargement automatique du package

EXEMPLE MÉTHODES :

typst_output <- function(x, ...) {
  UseMethod("typst_output")
}

typst_output.data.frame <- function(x, caption = NULL, label = NULL, ...) {
  # Génère : #rtable(data, headers, caption, label)
  generate_typst_table(x, caption, label)
}

typst_output.ggplot <- function(x, width = 6, height = 4, ...) {
  # Sauvegarde plot, retourne : #rplot("path.svg", ...)
}

DELIVERABLE : Tableaux data.frame/tibble formatés professionnellement

6.3 PHASE 3 : CACHE AVEC CHAÎNAGE (Semaines 9-12) ⭐
--------------------------------------------------------------------------------

OBJECTIF : Performance via cache intelligent avec invalidation en cascade

IMPLÉMENTATION :
  • Fichier cache.rs complet
  • Hash chaîné : Hash_n = sha256(Code_n + Options_n + Hash_{n-1} + deps)
  • Support option depends: [files...]
  • Gestion dépendances implicites (chaînage)
  • Gestion dépendances explicites (fichiers)
  • Metadata JSON pour indexation
  • Commandes knot clean, knot cache info

ALGORITHME :

fn compile_with_cache(&mut self, doc: &Document) -> Result<String> {
    let mut cache = Cache::new(PathBuf::from(".knot_cache"))?;
    let mut previous_hash = String::new();
    
    for (index, chunk) in doc.chunks.iter().enumerate() {
        // Calculer hash des dépendances
        let deps_hash = self.hash_dependencies(&chunk.options.depends)?;
        
        // Hash chaîné
        let chunk_hash = cache.get_chunk_hash(
            index,
            &chunk.code,
            &chunk.options,
            &previous_hash,
            &deps_hash
        );
        
        let result = if chunk.options.cache && cache.has_cached_result(&chunk_hash) {
            println!("  ✓ Chunk {} [cached]", chunk.name.as_deref().unwrap_or(&index.to_string()));
            cache.get_cached_result(&chunk_hash)?
        } else {
            println!("  ⚙ Chunk {} [executing]", chunk.name.as_deref().unwrap_or(&index.to_string()));
            let result = self.execute_chunk(chunk)?;
            cache.save_result(index, chunk.name.clone(), chunk_hash.clone(), &result)?;
            result
        };
        
        // Propager hash pour chaînage
        previous_hash = chunk_hash;
        
        // ... générer code Typst
    }
    
    Ok(generated_typst)
}

STRUCTURE Cache :

pub struct Cache {
    cache_dir: PathBuf,
    metadata: CacheMetadata,
}

pub struct CacheMetadata {
    pub document_hash: String,
    pub chunks: Vec<ChunkCacheEntry>,
}

pub struct ChunkCacheEntry {
    pub index: usize,
    pub name: Option<String>,
    pub hash: String,
    pub files: Vec<String>,
    pub dependencies: Vec<String>,
    pub updated_at: String,
}

MÉTHODES CLÉS :

impl Cache {
    pub fn get_chunk_hash(
        &self,
        chunk_index: usize,
        code: &str,
        options: &ChunkOptions,
        previous_hash: &str,
        dependencies_hash: &str,
    ) -> String {
        let chunk_content = format!(
            "{}|{}|{}|{}",
            code,
            serde_json::to_string(options).unwrap(),
            previous_hash,
            dependencies_hash
        );
        
        let mut hasher = Sha256::new();
        hasher.update(chunk_content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    pub fn has_cached_result(&self, hash: &str) -> bool { ... }
    pub fn get_cached_result(&self, hash: &str) -> Result<ExecutionResult> { ... }
    pub fn save_result(&mut self, ...) -> Result<()> { ... }
}

HASH DÉPENDANCES :

fn hash_dependencies(&self, depends: &[PathBuf]) -> Result<String> {
    let mut hasher = Sha256::new();
    
    for path in depends {
        if path.exists() {
            let metadata = fs::metadata(path)?;
            let modified = metadata.modified()?;
            
            // Hash: path + modified_time + size
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update(format!("{:?}", modified).as_bytes());
            hasher.update(metadata.len().to_string().as_bytes());
        } else {
            anyhow::bail!("Dependency not found: {:?}", path);
        }
    }
    
    Ok(format!("{:x}", hasher.finalize()))
}

ALTERNATIVE : Si fichier petit, hash contenu directement
fn hash_file_content(path: &Path) -> Result<String> {
    let content = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

TESTS CACHE :

#[test]
fn test_hash_chaining() {
    let cache = Cache::new(PathBuf::from("/tmp/test")).unwrap();
    
    let hash1 = cache.get_chunk_hash(0, "x <- 1", &opts, "", "");
    let hash2 = cache.get_chunk_hash(1, "y <- x + 1", &opts, &hash1, "");
    
    // Changer chunk 1 invalide chunk 2
    let hash1_mod = cache.get_chunk_hash(0, "x <- 2", &opts, "", "");
    let hash2_after = cache.get_chunk_hash(1, "y <- x + 1", &opts, &hash1_mod, "");
    
    assert_ne!(hash2, hash2_after);
}

#[test]
fn test_dependency_invalidation() {
    // Créer fichier test
    fs::write("/tmp/data.csv", "a,b\n1,2").unwrap();
    
    let opts = ChunkOptions {
        depends: vec![PathBuf::from("/tmp/data.csv")],
        ..Default::default()
    };
    
    let deps_hash1 = hash_dependencies(&opts.depends).unwrap();
    let hash1 = cache.get_chunk_hash(0, "x <- read_csv('data.csv')", &opts, "", &deps_hash1);
    
    // Modifier fichier
    thread::sleep(Duration::from_millis(10));
    fs::write("/tmp/data.csv", "a,b\n3,4").unwrap();
    
    let deps_hash2 = hash_dependencies(&opts.depends).unwrap();
    let hash2 = cache.get_chunk_hash(0, "x <- read_csv('data.csv')", &opts, "", &deps_hash2);
    
    assert_ne!(hash1, hash2);  // Invalidé !
}

DELIVERABLE : Recompilation rapide (~100ms pour cache hit complet)

6.4 PHASE 4 : GRAPHIQUES R (Semaines 13-16)
--------------------------------------------------------------------------------

OBJECTIF : Support plots R et ggplot2

NOTE (2026-01-11) : Deux approches possibles

APPROCHE 4A - EXPORT BITMAP/VECTORIEL (PRIORITÉ, décrit ci-dessous) :
  • Export SVG/PNG via ggsave() ou R devices
  • Simple, éprouvé, compatible tout R graphics
  • Utilise ExecutionResult::Plot existant

APPROCHE 4B - EXPORT NATIF TYPST (FUTUR, EXPLORATOIRE) :
  • Packages Typst : CeTZ ou Lilaq (https://lilaq.org)
  • Nécessiterait convertisseur ggplot2 → CeTZ/Lilaq
  • Avantages : vectoriel pur, éditable, thèmes cohérents
  • Complexité : parser grammaire ggplot2, projet massif
  • À considérer si convertisseur communautaire émerge

DÉCISION : Commencer par 4A (pragmatique ci-dessous)

IMPLÉMENTATION :
  • Redirection devices R (svg, png, pdf)
  • Méthode typst_output.ggplot()
  • Options fig-width, fig-height, fig-format
  • Cleanup automatique devices
  • Sauvegarde plots dans cache avec même hash

EXEMPLE GÉNÉRATION PLOT :

# Dans executor R
let plot_path = format!(".knot_cache/chunk_{}.svg", chunk_hash);
svg(filename = plot_path, width = 6, height = 4)
eval(user_code)
dev.off()

DELIVERABLE : Documents avec graphiques ggplot2 intégrés

6.5 PHASE 5 : PACKAGE TYPST (Semaines 17-20)
--------------------------------------------------------------------------------

OBJECTIF : Rendu professionnel et customisable

PACKAGE TYPST : @preview/knot

FONCTIONS PRINCIPALES :

#let rtable(data, headers, caption, label, ...) = { ... }
#let rcode(code, lang, numbers) = { ... }
#let routput(output, background, inset) = { ... }
#let rplot(path, caption, label, width, height) = { ... }

IMPORT AUTOMATIQUE :
  Le CLI injecte automatiquement :
  #import "@preview/knot:0.1.0": *

DELIVERABLE : Package publié sur @preview, rendu personnalisable

AMÉLIORATIONS FUTURES - PACKAGE TYPST (#code-chunk adaptive layout)
--------------------------------------------------------------------------------

NOTE (2026-01-11) : Le package Typst actuel utilise un layout 2 colonnes fixe. Il faudra rendre #code-chunk() intelligent selon le contexte :

LAYOUTS ADAPTATIFS :

1. CODE + RÉSULTAT TEXTE (actuel) :
   → Grid 2 colonnes (code | output)
   → Code avec coloration syntaxique
   → Output avec fond grisé

2. CODE SEUL (output: false) :
   → 1 colonne (pas de grid vide)
   → Code centré ou pleine largeur
   → Pas de fond grisé pour l'output absent

3. DATAFRAME/TABLE (via typst()) :
   → 1 colonne (même avec echo: true)
   → Code en haut si echo: true
   → Tableau Typst SANS fond grisé (intégré comme texte normal)
   → Mise en page fluide avec le texte environnant

IMPLÉMENTATION SUGGÉRÉE :

#let code-chunk(
  lang: none,
  name: none,
  echo: true,
  eval: true,
  input: none,
  output: none,
  caption: none,
  ..
) = {
  // Détecter le type d'output
  let has_output = output != none
  let is_table = false
  if has_output {
    // Heuristique : si output contient #table(...), c'est un DataFrame
    is_table = repr(output).contains("#table")
  }

  if is_table {
    // Layout 1 colonne pour DataFrame
    if echo { input }
    // Tableau sans fond, intégré au flux
    output
  } else if has_output {
    // Layout 2 colonnes pour texte
    grid(columns: (1fr, 1fr), gutter: 1em,
      input,
      block(fill: luma(244), radius: 4pt, inset: 8pt)[#output]
    )
  } else {
    // Layout 1 colonne pour code seul
    input
  }
}

BÉNÉFICES :
- Meilleure intégration visuelle des DataFrames
- Pas d'espace perdu pour code-only chunks
- Expérience utilisateur plus fluide
- Cohérent avec knitr/RMarkdown best practices

6.6 PHASE 6 : INLINE CHUNKS ET FINITIONS (Semaines 21-24)
--------------------------------------------------------------------------------

OBJECTIF : Feature complète v0.1.0

IMPLÉMENTATION :
  • Support #r-inline[expr] ou `r expr`
  • Formatage automatique valeurs numériques
  • Mode watch : knot watch document.knot
  • Configuration globale knot.toml
  • Toutes options chunks

DELIVERABLE : v0.1.0 utilisable en production

6.7 PHASE 7 : LILYPOND (Semaines 25-28)
--------------------------------------------------------------------------------

OBJECTIF : Support notation musicale

IMPLÉMENTATION :
  • LilyPondExecutor via subprocess
  • Template LilyPond automatique (paper block, version, etc.)
  • Options width, staff-size, format
  • Format SVG vectoriel
  • Cache plots LilyPond

EXEMPLE :

```{lilypond}
#| staff-size: 18
#| caption: "Gamme"
\relative c' { c d e f g a b c }
```

GÉNÉRATION AUTOMATIQUE :

let ly_content = format!(r#"\version \"2.24.0\"\n\paper {{\n  line-width = {{}}\\mm
}}\n# (set-global-staff-size {{}})
\score {{ {}}}
"#, width, staff_size, user_code);

// Compile: lilypond --svg output.ly

DELIVERABLE : Documents avec partitions musicales

6.8 PHASE 8 : TESTS ET STABILISATION (Semaines 29-32)
--------------------------------------------------------------------------------

OBJECTIF : Qualité production

  • Tests unitaires complets (parser, executors, cache)
  • Tests cache (chaînage, invalidation, dépendances)
  • Tests intégration (documents complets)
  • CI/CD GitHub Actions
  • Documentation complète
  • Exemples variés (stats, musique, maths)

DELIVERABLE : v0.5.0 stable et documentée

6.9 PHASE 9 : COMMUNAUTÉ (Semaines 33-40)
--------------------------------------------------------------------------------

OBJECTIF : Adoption et feedback

  • Publication GitHub/GitLab (PLMLab ?)
  • Annonces communautés (Typst, R, LilyPond)
  • Slogan "knot is not knitr" dans communication
  • Résolution bugs utilisateurs
  • Features demandées (priorité raisonnable)
  • Amélioration documentation

DELIVERABLE : v1.0.0 avec utilisateurs actifs

6.10 FUTUR (v2.0+)
--------------------------------------------------------------------------------

  • Python support (PyO3)
  • LSP pour édition interactive
  • Julia support (si demande)
  • Templates avancés (thesis, book, course)
  • Multi-documents
  • Bibliographie intégrée
  • knot cache gc (garbage collection des vieux caches)
  • Statistiques cache détaillées

================================================================================
7. SYSTÈME DE CACHE AVEC CHAÎNAGE (PHASE 3)
================================================================================

7.1 PRINCIPE FONDAMENTAL
--------------------------------------------------------------------------------

Hash_n = sha256(Code_n + Options_n + Hash_{n-1} + hash(dépendances))

Où :
  Code_n         : Code source du chunk n
  Options_n      : Options du chunk n (eval, echo, depends, etc.)
  Hash_{n-1}     : Hash du chunk précédent (chaînage)
  dépendances    : Hash des fichiers listés dans depends

7.2 AVANTAGES DU SYSTÈME
--------------------------------------------------------------------------------

1. INVALIDATION EN CASCADE AUTOMATIQUE
   Si Chunk 1 change → Hash_1 change → Hash_2 invalide → Hash_3 invalide
   Pas besoin d'analyser dépendances variables R

2. DÉPENDANCES IMPLICITES CAPTURÉES
   Variables passées entre chunks automatiquement gérées via chaînage

3. DÉPENDANCES EXPLICITES GÉRÉES
   Fichiers externes (CSV, R scripts) via option depends

4. DÉTERMINISME
   Même document → mêmes hash → comportement reproductible

5. PERFORMANCE
   Cache hit = pas d'exécution R (gain 100x-1000x)

7.3 EXEMPLES CONCRETS
--------------------------------------------------------------------------------

EXEMPLE 1 : Chaînage simple

```{r load}
#| eval: true
data <- read_csv("data.csv")
```

```{r process}
#| eval: true
summary <- data %>% summarise(mean = mean(value))
```

```{r plot}
#| eval: true
ggplot(data, aes(value)) + geom_histogram()
```

Cache :
  Hash_load    = sha256("read_csv..." + options + "")
  Hash_process = sha256("summarise..." + options + Hash_load)
  Hash_plot    = sha256("ggplot..." + options + Hash_process)

Si on change load → tout invalide
Si on change seulement plot → load et process restent cachés

EXEMPLE 2 : Dépendances explicites

```{r load}
#| eval: true
#| depends: data/raw.csv, scripts/clean.R
source("scripts/clean.R")
data <- read_csv("data/raw.csv") %>%
  clean_data()
```

```{r analyze}
#| eval: true
model <- lm(y ~ x, data)
```

Cache :
  deps_hash_load = sha256(raw.csv_mtime + clean.R_mtime)
  Hash_load      = sha256(code + options + "" + deps_hash_load)
  Hash_analyze   = sha256(code + options + Hash_load + "")

Si raw.csv change → deps_hash change → Hash_load invalide → Hash_analyze invalide
Si clean.R change → même effet
Si seulement analyze change → load reste caché

EXEMPLE 3 : Chunk non-eval

```{r setup}
#| eval: true
library(tidyverse)
x <- 1:10
```

```{r example}
#| eval: false
#| echo: true
# Ceci est un exemple (pas exécuté)
impossible_function()
```

```{r use-x}
#| eval: true
mean(x)
```

Cache :
  Hash_setup   = sha256(code + options + "")
  Hash_example = sha256(code + options + Hash_setup)  # Propagé même si pas exécuté
  Hash_use_x   = sha256(code + options + Hash_example)

Le chunk example n'est pas exécuté mais propage le hash pour maintenir chaîne.

7.4 IMPLÉMENTATION DÉTAILLÉE
--------------------------------------------------------------------------------

STRUCTURE CACHE :

.knot_cache/
├── metadata.json
├── chunk_abc123.txt
├── chunk_def456.svg
└── chunk_ghi789.txt

METADATA.JSON :

{
  "document_hash": "xyz...",
  "chunks": [
    {
      "index": 0,
      "name": "load-data",
      "hash": "abc123...",
      "files": ["chunk_abc123.txt"],
      "dependencies": ["data/raw.csv", "scripts/utils.R"],
      "updated_at": "2024-01-07T14:30:00Z"
    },
    {
      "index": 1,
      "name": "plot",
      "hash": "def456...",
      "files": ["chunk_def456.svg"],
      "dependencies": [],
      "updated_at": "2024-01-07T14:30:05Z"
    }
  ]
}

CODE RUST :

// cache.rs

use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::Result;

#[derive(Serialize, Deserialize)]
pub struct CacheMetadata {
    pub document_hash: String,
    pub chunks: Vec<ChunkCacheEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct ChunkCacheEntry {
    pub index: usize,
    pub name: Option<String>,
    pub hash: String,
    pub files: Vec<String>,
    pub dependencies: Vec<String>,
    pub updated_at: String,
}

pub struct Cache {
    cache_dir: PathBuf,
    metadata: CacheMetadata,
}

impl Cache {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&cache_dir)?;
        
        let metadata_path = cache_dir.join("metadata.json");
        let metadata = if metadata_path.exists() {
            let content = fs::read_to_string(&metadata_path)?;
            serde_json::from_str(&content)?
        } else {
            CacheMetadata {
                document_hash: String::new(),
                chunks: Vec::new(),
            }
        };
        
        Ok(Self { cache_dir, metadata })
    }
    
    pub fn get_chunk_hash(
        &self,
        chunk_index: usize,
        code: &str,
        options: &ChunkOptions,
        previous_hash: &str,
        dependencies_hash: &str,
    ) -> String {
        let chunk_content = format!(
            "{}|{}|{}|{}",
            code,
            serde_json::to_string(options).unwrap(),
            previous_hash,
            dependencies_hash
        );
        
        let mut hasher = Sha256::new();
        hasher.update(chunk_content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    pub fn has_cached_result(&self, hash: &str) -> bool {
        self.metadata.chunks.iter()
            .any(|entry| entry.hash == hash)
    }
    
    pub fn get_cached_result(&self, hash: &str) -> Result<ExecutionResult> {
        let entry = self.metadata.chunks.iter()
            .find(|e| e.hash == hash)
            .ok_or_else(|| anyhow::anyhow!("Cache entry not found"))?;
        
        // Vérifier que fichiers existent
        for file in &entry.files {
            let path = self.cache_dir.join(file);
            if !path.exists() {
                anyhow::bail!("Cache file missing: {:?}", path);
            }
        }
        
        // Charger résultat
        let result_path = self.cache_dir.join(&entry.files[0]);
        let ext = result_path.extension().and_then(|e| e.to_str());
        
        match ext {
            Some("txt") => {
                let text = fs::read_to_string(&result_path)?;
                Ok(ExecutionResult::Text(text))
            }
            Some("svg") | Some("png") | Some("pdf") => {
                Ok(ExecutionResult::Plot(result_path))
            }
            _ => anyhow::bail!("Unknown cache file type")
        }
    }
    
    pub fn save_result(
        &mut self,
        chunk_index: usize,
        chunk_name: Option<String>,
        hash: String,
        result: &ExecutionResult,
        dependencies: Vec<PathBuf>,
    ) -> Result<()> {
        let files = match result {
            ExecutionResult::Text(text) => {
                let filename = format!("chunk_{}.txt", hash);
                let path = self.cache_dir.join(&filename);
                fs::write(&path, text)?;
                vec![filename]
            }
            ExecutionResult::Plot(plot_path) => {
                vec![plot_path.file_name().unwrap().to_string_lossy().to_string()]
            }
            ExecutionResult::Both { text, plot } => {
                let text_file = format!("chunk_{}.txt", hash);
                fs::write(self.cache_dir.join(&text_file), text)?;
                vec![
                    text_file,
                    plot.file_name().unwrap().to_string_lossy().to_string()
                ]
            }
        };
        
        self.metadata.chunks.push(ChunkCacheEntry {
            index: chunk_index,
            name: chunk_name,
            hash,
            files,
            dependencies: dependencies.iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
        
        self.save_metadata()?;
        Ok(())
    }
    
    fn save_metadata(&self) -> Result<()> {
        let metadata_path = self.cache_dir.join("metadata.json");
        let content = serde_json::to_string_pretty(&self.metadata)?;
        fs::write(metadata_path, content)?;
        Ok(())
    }
}

pub fn hash_dependencies(depends: &[PathBuf]) -> Result<String> {
    if depends.is_empty() {
        return Ok(String::new());
    }
    
    let mut hasher = Sha256::new();
    
    for path in depends {
        if !path.exists() {
            anyhow::bail!("Dependency not found: {:?}", path);
        }
        
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;
        
        // Hash: path + modified_time + size
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(format!("{:?}", modified).as_bytes());
        hasher.update(metadata.len().to_string().as_bytes());
    }
    
    Ok(format!("{:x}", hasher.finalize()))
}

7.5 UTILISATION DANS COMPILER
--------------------------------------------------------------------------------

// compiler.rs

pub fn compile(&mut self, doc: &Document) -> Result<String> {
    let mut cache = Cache::new(PathBuf::from(".knot_cache"))?;
    let mut codegen = CodeGenerator::new();
    let mut previous_hash = String::new();
    
    // Initialiser executors
    if let Some(ref mut r_exec) = self.r_executor {
        r_exec.initialize()?;
    }
    
    println!("Compiling {} chunks...", doc.chunks.len());
    
    for (index, chunk) in doc.chunks.iter().enumerate() {
        let chunk_name = chunk.name.as_deref()
            .unwrap_or(&format!("chunk-{}", index));
        
        // Hash des dépendances
        let deps_hash = hash_dependencies(&chunk.options.depends)?;
        
        // Hash chaîné du chunk
        let chunk_hash = cache.get_chunk_hash(
            index,
            &chunk.code,
            &chunk.options,
            &previous_hash,
            &deps_hash
        );
        
        let result = if !chunk.options.eval {
            // Chunk non-eval : propager hash mais ne pas exécuter
            previous_hash = chunk_hash;
            
            if chunk.options.echo {
                let chunk_output = format_code_block(&chunk.code);
                codegen.add_chunk_result(chunk_output);
            }
            continue;
            
        } else if chunk.options.cache && cache.has_cached_result(&chunk_hash) {
            // Cache hit
            println!("  ✓ {} [cached]", chunk_name);
            cache.get_cached_result(&chunk_hash)?
            
        } else {
            // Cache miss : exécuter
            println!("  ⚙ {} [executing]", chunk_name);
            let result = self.execute_chunk(chunk)?;
            cache.save_result(
                index,
                chunk.name.clone(),
                chunk_hash.clone(),
                &result,
                chunk.options.depends.clone()
            )?;
            result
        };
        
        // Générer code Typst
        let mut chunk_output = String::new();
        
        if chunk.options.echo {
            chunk_output.push_str(&format_code_block(&chunk.code));
            chunk_output.push('\n');
        }
        
        if chunk.options.output {
            chunk_output.push_str(&format_result(&result));
        }
        
        codegen.add_chunk_result(chunk_output);
        
        // Propager hash pour chaînage
        previous_hash = chunk_hash;
    }
    
    codegen.generate(doc)
}

7.6 TESTS CACHE
--------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    
    #[test]
    fn test_hash_chaining_basic() {
        let cache = Cache::new(PathBuf::from("/tmp/test_knot")).unwrap();
        let opts = ChunkOptions::default();
        
        let hash1 = cache.get_chunk_hash(0, "x <- 1", &opts, "", "");
        let hash2 = cache.get_chunk_hash(1, "y <- x + 1", &opts, &hash1, "");
        let hash3 = cache.get_chunk_hash(2, "z <- y * 2", &opts, &hash2, "");
        
        // Changer chunk 1 invalide tout
        let hash1_mod = cache.get_chunk_hash(0, "x <- 2", &opts, "", "");
        let hash2_after = cache.get_chunk_hash(1, "y <- x + 1", &opts, &hash1_mod, "");
        let hash3_after = cache.get_chunk_hash(2, "z <- y * 2", &opts, &hash2_after, "");
        
        assert_ne!(hash1, hash1_mod);
        assert_ne!(hash2, hash2_after);
        assert_ne!(hash3, hash3_after);
    }
    
    #[test]
    fn test_dependency_invalidation() {
        let tmp_file = "/tmp/knot_test_data.csv";
        fs::write(tmp_file, "a,b\n1,2").unwrap();
        
        let opts = ChunkOptions {
            depends: vec![PathBuf::from(tmp_file)],
            ..Default::default()
        };
        
        let deps_hash1 = hash_dependencies(&opts.depends).unwrap();
        let hash1 = Cache::new(PathBuf::from("/tmp/test_knot"))
            .unwrap()
            .get_chunk_hash(0, "read.csv('data.csv')", &opts, "", &deps_hash1);
        
        // Modifier fichier
        thread::sleep(Duration::from_millis(10));
        fs::write(tmp_file, "a,b\n3,4").unwrap();
        
        let deps_hash2 = hash_dependencies(&opts.depends).unwrap();
        let hash2 = Cache::new(PathBuf::from("/tmp/test_knot"))
            .unwrap()
            .get_chunk_hash(0, "read.csv('data.csv')", &opts, "", &deps_hash2);
        
        assert_ne!(deps_hash1, deps_hash2);  // Invalidé !
        assert_ne!(hash1, hash2);
        
        // Cleanup
        fs::remove_file(tmp_file).unwrap();
    }
    
    #[test]
    fn test_options_affect_hash() {
        let cache = Cache::new(PathBuf::from("/tmp/test_knot")).unwrap();
        
        let opts1 = ChunkOptions { eval: true, ..Default::default() };
        let opts2 = ChunkOptions { eval: false, ..Default::default() };
        
        let hash1 = cache.get_chunk_hash(0, "x <- 1", &opts1, "", "");
        let hash2 = cache.get_chunk_hash(0, "x <- 1", &opts2, "", "");
        
        assert_ne!(hash1, hash2);
    }
}

7.7 COMMANDES CACHE
--------------------------------------------------------------------------------

knot clean
  Supprime .knot_cache/ complètement
  Force ré-exécution de tous les chunks

knot clean --keep-metadata
  Supprime fichiers cache mais garde metadata.json
  Utile pour inspection

knot cache info
  Affiche statistiques cache :
    - Nombre de chunks cachés
    - Taille totale cache
    - Ancienneté du cache
  
  Exemple output :
    Cache directory: .knot_cache/
    Total chunks: 12
    Cache size: 2.4 MB
    Oldest entry: 2 days ago
    Newest entry: 5 minutes ago

knot cache list
  Liste tous les chunks cachés avec détails
  
  Exemple output :
    Chunk 0 (load-data)
      Hash: abc123...
      Files: chunk_abc123.txt
      Dependencies: data/raw.csv
      Updated: 2024-01-07 14:30:00
    
    Chunk 1 (plot)
      Hash: def456...
      Files: chunk_def456.svg
      Dependencies: none
      Updated: 2024-01-07 14:30:05

knot compile --no-cache
  Force ré-exécution de tous les chunks (ignore cache)

knot compile --force
  Alias pour --no-cache

================================================================================
8. PATTERNS DE CODE IMPORTANTS
================================================================================

8.1 PARSER (PHASE 1)
--------------------------------------------------------------------------------

fn extract_chunks(source: &str) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    
    // Regex pour ```{r} ou ```{r chunk-name}
    let re = Regex::new(
        r"(?s)```\{(r|python|lilypond)\s*([^\}]*)\}
((?://\|.*\n)*)(.*?)```"
    ).context("Failed to compile regex")?;
    
    for cap in re.captures_iter(source) {
        // NOTE: Pour un vrai LSP, il faudrait convertir les offsets en bytes
        // du match regex en positions (ligne, colonne) pour plus de précision.
        // Ce qui suit est une simplification.

        let language = cap[1].to_string();
        let chunk_name = cap[2].trim().to_string();
        let options_block = &cap[3];
        let code = cap[4].trim().to_string();
        
        let options = parse_options(options_block)?;

        // TODO: Calculer les vraies positions (range, code_range)
        let dummy_pos = Position { line: 0, column: 0 };
        let dummy_range = Range { start: dummy_pos.clone(), end: dummy_pos.clone() };
        
        chunks.push(Chunk {
            language,
            name: if chunk_name.is_empty() { 
                None 
            } else { 
                Some(chunk_name) 
            },
            code,
            options,
            range: dummy_range.clone(),
            code_range: dummy_range.clone(),
        });
    }
    
    Ok(chunks)
}

8.2 OPTIONS PARSING AVEC DEPENDS
--------------------------------------------------------------------------------

fn parse_options(options_block: &str) -> Result<ChunkOptions> {
    let mut options = ChunkOptions::default();
    
    for line in options_block.lines() {
        let line = line.trim();
        if !line.starts_with("#|") {
            continue;
        }
        
        let option_str = line.trim_start_matches("#|").trim();
        
        if let Some((key, value)) = option_str.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            
            match key {
                "eval" => options.eval = parse_bool(value)?,
                "echo" => options.echo = parse_bool(value)?,
                "output" => options.output = parse_bool(value)?,
                "cache" => options.cache = parse_bool(value)?,
                "label" => options.label = Some(value.to_string()),
                "caption" => options.caption = Some(value.to_string()),
                "depends" => {
                    // Parse liste fichiers séparés par virgules
                    options.depends = value
                        .split(',')
                        .map(|s| PathBuf::from(s.trim()))
                        .collect();
                }
                _ => {} // Ignorer options inconnues
            }
        }
    }
    
    Ok(options)
}

fn parse_bool(s: &str) -> Result<bool> {
    match s.to_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => anyhow::bail!("Invalid boolean value: {}", s),
    }
}

8.3 EXECUTOR R (SUBPROCESS PHASE 1)
--------------------------------------------------------------------------------

use std::process::Command;
use std::io::Write;
use tempfile::NamedTempFile;

pub struct RExecutor {
    cache_dir: PathBuf,
}

impl RExecutor {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }
}

impl LanguageExecutor for RExecutor {
    fn initialize(&mut self) -> Result<()> {
        // Vérifier que R est installé
        Command::new("R")
            .arg("--version")
            .output()
            .context("R not found. Please install R.")?;
        Ok(())
    }
    
    fn execute(&mut self, code: &str) -> Result<ExecutionResult> {
        // Créer fichier temporaire avec le code
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "{}", code)?;
        let temp_path = temp_file.path();
        
        // Exécuter R --vanilla --quiet < temp.R
        let output = Command::new("R")
            .arg("--vanilla")
            .arg("--quiet")
            .arg("--no-save")
            .stdin(std::fs::File::open(temp_path)?)
            .output()
            .context("Failed to execute R")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "R execution failed:\n\n        Code:\n```r\n{}\n```\n\n        Error:\n{}",
                code, stderr
            );
        }
        
        let result_text = String::from_utf8_lossy(&output.stdout).to_string();
        
        Ok(ExecutionResult::Text(result_text))
    }
}

8.4 COMPILER AVEC CACHE (PHASE 3)
--------------------------------------------------------------------------------

// Voir section 7.5 pour implémentation complète

8.5 CODE GENERATOR
--------------------------------------------------------------------------------

use crate::parser::Document;
use regex::Regex;
use anyhow::Result;

pub struct CodeGenerator {
    compiled_chunks: Vec<String>,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self { compiled_chunks: Vec::new() }
    }
    
    pub fn add_chunk_result(&mut self, result: String) {
        self.compiled_chunks.push(result);
    }
    
    pub fn generate(&self, doc: &Document) -> Result<String> {
        let mut output = doc.source.clone();
        let re = Regex::new(
            r"(?s)```\{(r|python|lilypond)\s*([^\}]*)\}
((?://\|.*\n)*)(.*?)```"
        )?;
        
        let mut chunk_idx = 0;
        let result = re.replace_all(&output, |_caps: &regex::Captures| {
            let replacement = if chunk_idx < self.compiled_chunks.len() {
                self.compiled_chunks[chunk_idx].clone()
            } else {
                String::new()
            };
            chunk_idx += 1;
            replacement
        });
        
        Ok(result.to_string())
    }
}

8.6 CLI COMPILE
--------------------------------------------------------------------------------

fn compile(input: &PathBuf, output: Option<&PathBuf>) -> Result<()> {
    use anyhow::Context;
    
    println!("📄 Compiling {:?}...", input);
    
    // Lire et parser
    let source = fs::read_to_string(input)
        .context(format!("Failed to read {:?}", input))?;
    
    let doc = knot_core::Document::parse(source)
        .context("Failed to parse document")?;
    
    println!("✓ Parsed {} chunk(s)", doc.chunks.len());
    
    // Compiler chunks (avec cache en Phase 3)
    let mut compiler = knot_core::Compiler::new()
        .context("Failed to initialize compiler")?;
    
    println!("🔧 Executing code chunks...");
    
    let typst_code = compiler.compile(&doc)
        .context("Failed to compile chunks")?;
    
    println!("✓ All chunks executed successfully");
    
    // Générer .typ
    let typ_output = input.with_extension("typ");
    fs::write(&typ_output, typst_code)?;
    println!("✓ Generated {:?}", typ_output);
    
    // Compiler avec typst
    let pdf_output = output
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| input.with_extension("pdf"));
    
    let status = std::process::Command::new("typst")
        .arg("compile")
        .arg(&typ_output)
        .arg(&pdf_output)
        .status()?;
    
    if !status.success() {
        anyhow::bail!("Typst compilation failed");
    }
    
    println!("✓ Compiled to {:?}", pdf_output);
    
    Ok(())
}

================================================================================
9. EXEMPLES DE DOCUMENTS
================================================================================

9.1 DOCUMENT SIMPLE
--------------------------------------------------------------------------------

#set page(width: 210mm, margin: 25mm)
#set text(font: "New Computer Modern", size: 11pt)

= Mon Analyse

```{r setup}
#| eval: true
#| echo: true
library(tidyverse)
data <- iris
```

```{r summary}
#| eval: true
#| output: true
summary(data$Sepal.Length)
```

9.2 DOCUMENT AVEC DÉPENDANCES
--------------------------------------------------------------------------------

```{r load-data}
#| eval: true
#| echo: true
#| depends: data/raw.csv, scripts/utils.R
source("scripts/utils.R")
data <- read_csv("data/raw.csv") %>%
  clean_data()
```

```{r analyze}
#| eval: true
model <- lm(y ~ x, data)
summary(model)
```

Si data/raw.csv ou scripts/utils.R change → chunks ré-exécutés automatiquement

9.3 DOCUMENT AVEC GRAPHIQUE (PHASE 4+)
--------------------------------------------------------------------------------

```{r plot}
#| eval: true
#| echo: false
#| fig-width: 6
#| fig-height: 4
#| label: fig-scatter
ggplot(iris, aes(Sepal.Length, Sepal.Width, color = Species)) +
  geom_point() +
  theme_minimal()
```

Voir @fig-scatter pour les résultats.

9.4 DOCUMENT AVEC LILYPOND (PHASE 7+)
--------------------------------------------------------------------------------

```{lilypond}
#| eval: true
#| staff-size: 18
#| caption: "Gamme de Do majeur"
\relative c' {
  \time 4/4
  c4 d e f | g a b c
}
```

9.5 TEMPLATE KNOT INIT
--------------------------------------------------------------------------------

// Generated by knot
// "knot is not knitr"

#set page(width: 210mm, margin: 25mm)
#set text(font: "New Computer Modern", size: 11pt, lang: "en")
#set par(justify: true)

#align(center)[
  #text(size: 18pt, weight: "bold")[
    My Knot Document
  ]
  
  #v(0.5em)
  
  Your Name
  
  #datetime.today().display()
]

#v(2em)

= Introduction

This is a knot document with executable R code.

== Example chunk

```{r}
#| eval: true
#| echo: true
#| output: true

# Load data
data <- iris

# Compute summary statistics
summary(data$Sepal.Length)
```

== Example with dependencies

```{r}
#| eval: true
#| depends: data/input.csv
# This chunk will re-execute if input.csv changes
data <- read_csv("data/input.csv")
```

== Next steps

Edit this file and add your own code chunks!

Try: knot compile document.knot

================================================================================
10. CRITÈRES DE SUCCÈS
================================================================================

10.1 PHASE 1
--------------------------------------------------------------------------------
✅ knot compile demo.knot produit PDF correct
✅ Code R exécuté, résultats visibles dans PDF
✅ Options eval, echo, output fonctionnent correctement
✅ Erreurs R capturées et affichées proprement
✅ cargo test passe tous les tests

10.2 PHASE 3 (CACHE)
--------------------------------------------------------------------------------
✅ Cache fonctionne avec chaînage
✅ Invalidation en cascade automatique
✅ Dépendances explicites (depends) gérées
✅ Recompilation rapide (cache hit ~100ms)
✅ knot clean fonctionne
✅ Tests cache passent

10.3 V1.0
--------------------------------------------------------------------------------
✅ Cache intelligent complet
✅ Graphiques ggplot2 intégrés
✅ Package Typst publié sur @preview
✅ Inline R expressions fonctionnelles
✅ Support LilyPond complet
✅ Documentation complète et exemples
✅ Slogan "knot is not knitr" présent dans communication
✅ Utilisateurs actifs et feedback positif

================================================================================
11. POINTS D'ATTENTION
================================================================================

11.1 SIMPLIFICATIONS PHASE 1 (ACCEPTABLES)
--------------------------------------------------------------------------------
• Subprocess R au lieu d'extendr embedded
• PAS DE CACHE (implémenté en Phase 3)
• Pas de graphiques
• Pas d'inline expressions (#r-inline[])
• Génération Typst basique (pas de #rtable(), etc.)

Ces limitations seront levées dans phases suivantes.

11.2 DÉCISIONS À PRENDRE EN DÉVELOPPANT
--------------------------------------------------------------------------------
• Hash dépendances : mtime+size vs contenu complet ?
• Garbage collection cache : automatique ou manuel ?
• Cache partagé entre documents ou isolé ?
• Format sérialisation résultats : JSON vs bincode ?
• Gestion erreurs LilyPond : montrer stderr complet ou filtrer ?

11.3 ARCHITECTURE ÉVOLUTIVE
--------------------------------------------------------------------------------
• Séparation knot-core (lib) et knot-cli dès le début
• Permet ajout LSP en v2.0 sans refonte majeure
• Trait LanguageExecutor facilite ajout Python/Julia
• Structure workspace facilite maintenance multi-composants

• Système cache extensible (autres stratégies possibles)

11.4 POINT D'ATTENTION : PRÉPARATION POUR LE LSP
--------------------------------------------------------------------------------
Pour faciliter l'implémentation future d'un LSP (Phase 9+), le parser
doit capturer des informations de position détaillées.
• La structure `Chunk` a été enrichie pour inclure `range` et `code_range`.
• `range` capture la position de tout le chunk (de ```{lang} à ```).
• `code_range` capture la position du code source seul.
• Ceci est essentiel pour mapper les diagnostics et fournir des fonctionnalités
  comme le "hover" ou "go to definition".
• À long terme, un parser tolérant aux erreurs (ex: `nom` ou `chumsky`) 
  serait préférable à une approche purement regex.

================================================================================
12. DIFFÉRENCIATION VS ALTERNATIVES
================================================================================

Outil         Input       Backend     Vitesse      Cache        Contrôle
-------------
knitr         R Markdown  LaTeX       ⭐⭐          ⭐⭐⭐        ⭐⭐
Quarto        Markdown    Pandoc      ⭐⭐          ⭐⭐⭐        ⭐⭐
Lilaq         Markdown    Typst       ⭐⭐⭐⭐      ?            ⭐⭐⭐
Jupyter       Notebook    Kernel      ⭐⭐⭐        ❌           ⭐
KNOT          Typst       Typst       ⭐⭐⭐⭐⭐    ⭐⭐⭐⭐⭐    ⭐⭐⭐⭐⭐

POSITIONNEMENT :
"knot is not knitr" - Alternative moderne pour académiques voulant
qualité typographique + performance + reproductibilité

AVANTAGES UNIQUES DE KNOT :
• Cache avec chaînage SHA256 (invalidation cascade automatique)
• Dépendances explicites fichiers (depends)
• Exécution strictement linéaire (reproductible)
• Performance maximale (Rust + Typst)
• Typst natif (contrôle typo total)

NICHE CIBLE :
• Statisticiens/mathématiciens (R + LaTeX users)
• Musicologues (LilyPond users)
• Académiques exigeants sur la qualité typographique
• Utilisateurs Typst voulant literate programming

================================================================================
13. ESTIMATION TEMPS
================================================================================

DÉVELOPPEMENT TEMPS PARTIEL (10h/semaine) :

Phase     Description              Durée
---------
Phase 1   R basique               1 mois
Phase 2   Package R               1 mois
Phase 3   Cache chaîné ⭐         1 mois
Phase 4   Graphiques              1 mois
Phase 5   Package Typst           1 mois
Phase 6   Inline + finitions      1 mois
Phase 7   LilyPond                1 mois
Phase 8   Tests & stabilisation   1 mois
Phase 9   Communauté & feedback   2 mois

TOTAL V1.0 : ~10 mois

ACCÉLÉRATEURS POTENTIELS :
• Contributions communauté (après v0.1)
• Réutilisation code existant (extendr, tower-lsp)
• Scope discipliné (résister au feature creep)
• Architecture propre facilite ajouts

================================================================================
14. PROCHAINE ÉTAPE IMMÉDIATE
================================================================================

COMMENCER PHASE 1, SEMAINE 1, JOUR 1-2 : Setup projet

ACTIONS :

1. VÉRIFIER DISPONIBILITÉ NOM "knot"
   cargo search knot
   # Vérifier aussi GitHub, npm, PyPI
   # Check @preview Typst packages

2. INITIALISER WORKSPACE CARGO
   mkdir knot && cd knot
   cargo new --lib crates/knot-core
   cargo new crates/knot-cli

3. CRÉER WORKSPACE Cargo.toml
   [workspace]
   members = ["crates/*"]
   resolver = "2"
   
   [workspace.dependencies]
   knot-core = { path = "crates/knot-core" }
   clap = { version = "4.5", features = ["derive"] }
   regex = "1.10"
   anyhow = "1.0"
   tempfile = "3.8"
   sha2 = "0.10"
   serde = { version = "1.0", features = ["derive"] }
   serde_json = "1.0"

4. AJOUTER DÉPENDANCES
   cd crates/knot-core
   cargo add anyhow regex tempfile sha2 serde --features serde/derive
   cargo add serde_json
   
   cd ../knot-cli
   cargo add clap --features derive
   cargo add knot-core --path ../knot-core

5. VÉRIFIER COMPILATION
   cargo build
   # Doit compiler sans erreurs

6. INITIALISER GIT
   git init
   git add .
   git commit -m "Initial commit: knot workspace setup"

7. CRÉER .gitignore
   
   # Rust
   /target/
   **/*.rs.bk
   Cargo.lock
   
   # Knot
   .knot_cache/
   *.typ
   *.pdf
   
   # R
   .Rhistory
   .RData
   
   # OS
   .DS_Store

8. CRÉER README.md
   # Knot
   
   **knot is not knitr**
   
   Literate programming for the Typst era.
   
   ## Status
   
   Phase 1: Prototype in development
   
   ## Quick Start
   
   Coming soon...

PUIS CONTINUER AVEC JOUR 3-4 : Parser minimal

================================================================================
FIN DU DOCUMENT DE RÉFÉRENCE
================================================================================

Ce document sert de référence complète pour le développement de knot avec
Claude Code. Il résume 3+ heures de discussion architecturale et de décisions
de design.

"knot is not knitr" - Une alternative moderne pour literate programming

Dernière mise à jour : Janvier 2024
Statut : Projet en phase de conception, prêt pour Phase 1

---

# PART 2: LSP ARCHITECTURE DETAIL

(Originally `LSP-ARCHITECTURE.md`)

# Architecture LSP Knot avec Proxy Tinymist

## Contexte et décision

### Problème initial
Notre première implémentation (serveur LSP standalone pour .knot) réinventait la roue :
- ❌ Pas de support Typst natif (hover, completion, diagnostics)
- ❌ Pas de live preview
- ❌ Maintenance lourde (suivre les évolutions Typst)
- ❌ Expérience utilisateur limitée

### Solution retenue : Proxy LSP avec tinymist
Au lieu de créer un serveur LSP complet, nous créons un **proxy intelligent** qui :
- ✅ Délègue tout le support Typst à tinymist (serveur LSP mature)
- ✅ Ajoute uniquement les fonctionnalités spécifiques à knot
- ✅ Bénéficie du live preview de tinymist
- ✅ Minimise la duplication de code

---

## Architecture globale

```
┌─────────────────────────────────────────────────────────────────┐
│                        VS Code (User)                            │
│                    Extension knot (.knot files)                  │
└────────────────────────────┬────────────────────────────────────┘
                             │ LSP (stdio)
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                         knot-lsp (Proxy)                         │
├─────────────────────────────────────────────────────────────────┤
│  • Reçoit requêtes LSP du client                                │
│  • Maintient état des documents .knot ouverts                   │
│  • Gère deux flux parallèles :
│                                                                  │
│  [Flux 1: Analyse Typst]                                        │
│    ├─ Transform .knot → .typ factice (supprime chunks R)        │
│    ├─ Forward à tinymist subprocess                             │
│    └─ Reçoit diagnostics/hover/completion Typst                 │
│                                                                  │
│  [Flux 2: Analyse Knot]                                         │
│    ├─ Parse chunks R (parser.rs)                                │
│    ├─ Génère diagnostics knot-spécifiques                       │
│    └─ Génère symbols (outline chunks)                           │
│                                                                  │
│  • Merge les résultats (diagnostics Typst + diagnostics Knot)   │
│  • Adapte les positions (mapping .knot ↔ .typ)                  │
│  • Renvoie résultat combiné au client                           │
└────────────────────────────┬────────────────────────────────────┘
                             │ LSP (stdio/TCP)
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    tinymist (Subprocess)                         │
├─────────────────────────────────────────────────────────────────┤
│  • Spawné et géré par knot-lsp                                  │
│  • Reçoit .typ factice (sans chunks R)                          │
│  • Fournit :
│    - Diagnostics Typst (syntaxe, typage)                        │
│    - Hover sur fonctions/variables Typst                        │
│    - Completion Typst                                            │
│    - Live preview de la structure                               │
└─────────────────────────────────────────────────────────────────┘
```

---

## Transformation .knot → .typ factice

### Objectif
Produire un document Typst **syntaxiquement valide** en **< 10ms** pour que tinymist puisse l'analyser, SANS exécuter de code R.

### Transformation rapide

#### Entrée (.knot)
```knot
# Analyse des données

```{r} 
library(tidyverse)
df <- read.csv("data.csv")
```

Le dataset contient #r[nrow(df)] observations.

```{r}
#| fig-width: 10
ggplot(df, aes(x, y)) + geom_point()
```

La moyenne est #r[mean(df$x)].
```

#### Sortie (.typ factice)
```typ
# Analyse des données



Le dataset contient ? observations.



La moyenne est ?.
```

### Algorithme de transformation

```rust
fn transform_knot_to_typst_placeholder(knot_content: &str) -> String {
    // 1. Supprimer les chunks R (regex CHUNK_REGEX)
    let without_chunks = CHUNK_REGEX.replace_all(knot_content, "");

    // 2. Remplacer expressions inline #r[...] par "?"
    let without_inline = replace_inline_expressions(&without_chunks);

    without_inline.to_string()
}
```

### Caractéristiques
- ⚡ **Instantané** (< 10ms) - simple regex, pas d'exécution
- ✅ **Syntaxe Typst valide** - tinymist ne voit pas d'erreurs de syntaxe
- 🎯 **Préserve structure** - titres, paragraphes, markup Typst intact
- 🔄 **Idempotent** - peut être refait à chaque frappe

---

## Système de preview à deux niveaux

### Niveau 1 : Preview immédiat (Structure)

**Déclencheur** : Chaque modification du document
**Délai** : < 100ms
**Processus** :
1. Transformation .knot → .typ factice (instantanée)
2. tinymist affiche le preview de structure
3. Résultats R affichés comme `?` ou `⟳ Computing...`

**Ce que l'utilisateur voit** :
- ✅ Mise en page Typst en temps réel
- ✅ Titres, texte, markup
- ⏳ Placeholders pour résultats R

### Niveau 2 : Preview complet (Résultats)

**Déclencheur** : Modification détectée (avec debouncing)
**Délai** : Variable (dépend du code R)
**Processus** :
1. Debounce 500ms (attendre que l'utilisateur finisse de taper)
2. Lancer `knot compile` en background avec **cache Phase 3**
3. Produire .typ final avec vrais résultats R
4. tinymist met à jour le preview

**Ce que l'utilisateur voit** :
- 🔄 Indicateur "Compiling chunks..." pendant l'exécution
- 📊 Preview se met à jour progressivement (chunk par chunk)
- ✅ Résultats finaux (tableaux, graphiques, valeurs)

### Workflow combiné

```
Utilisateur tape "x <- 2"
    ↓ [< 100ms]
Preview structure (montre "?")
    ↓ [debounce 500ms]
    ↓ [compile avec cache]
    ↓ [2s si cache hit, plus si cache miss]
Preview complet (montre "2")
```

### Optimisations avec le cache

Le système de cache existant (Phase 3) est **crucial** :

#### Cache hit (chunk non modifié)
```
Chunk 1 modifié → recompile (2s)
Chunk 2 identique → cache (< 10ms) ✅
Chunk 3 identique → cache (< 10ms) ✅
Chunk 4 identique → cache (< 10ms) ✅

Total : ~2s au lieu de 8s
```

#### Invalidation intelligente
```
Chunk 1 : x <- data.csv (MODIFIÉ)
Chunk 2 : mean(x) (INVALIDE - dépend de chunk 1)
Chunk 3 : y <- other.csv (CACHE HIT - indépendant)
Chunk 4 : mean(y) (CACHE HIT - dépend uniquement de chunk 3)
```

---

## Fonctionnalités LSP par source

### Fournies par tinymist (déléguées)
- ✅ Diagnostics syntaxe Typst
- ✅ Hover sur fonctions Typst (`#table`, `#figure`, etc.)
- ✅ Completion Typst (fonctions, symboles, packages)
- ✅ Jump to definition (symboles Typst)
- ✅ Formatting Typst
- ✅ Live preview de structure

### Fournies par knot-lsp (spécifiques)
- ✅ Diagnostics chunks R
  - Chunk mal formé (` ```{r} ` sans closing)
  - Options invalides (`#| unknown-option: value`)
  - Inline expressions mal formées (`#r[unmatched`)
- ✅ Document symbols
  - Outline des chunks R dans la sidebar
  - Navigation rapide entre chunks
- ✅ Hover sur chunks
  - Options actives (eval, echo, cache, etc.)
  - Dépendances du chunk
  - État du cache
- ✅ Completion knot
  - Options de chunks après `#|` (eval, echo, output, cache, fig-width, dpi, etc.)
  - Noms de chunks pour références
  - Langages supportés (r, python, lilypond)

### Combinées (merge)
- 🔀 **Diagnostics** : tinymist (Typst) + knot-lsp (chunks R)
- 🔀 **Symbols** : tinymist (symboles Typst) + knot-lsp (chunks R)

---

## Mapping des positions

### Problème
Les positions changent entre .knot et .typ factice (chunks supprimés).

#### Exemple
```knot
Line 1: # Titre
Line 2:
Line 3: ```{r}
Line 4: x <- 1
Line 5: ```
Line 6:
Line 7: Résultat: #r[x]
```

Devient :
```typ
Line 1: # Titre
Line 2:
Line 3: (vide - chunk supprimé)
Line 4: (vide - chunk supprimé)
Line 5: (vide - chunk supprimé)
Line 6:
Line 7: Résultat: ?
```

**Diagnostic tinymist sur line 7** doit être mappé à **line 7 du .knot original**.

### Solution : Position mapping table

```rust
struct PositionMapper {
    // Mapping ligne knot → ligne typ
    knot_to_typ: HashMap<usize, usize>,

    // Mapping ligne typ → ligne knot
    typ_to_knot: HashMap<usize, usize>,

    // Régions supprimées (chunks R)
    removed_regions: Vec<Range>,
}

impl PositionMapper {
    fn new(knot_content: &str, chunks: &[Chunk]) -> Self {
        // Construire les mappings en analysant les chunks
        // ...
    }

    fn map_position_typ_to_knot(&self, typ_pos: Position) -> Position {
        // Convertir position dans .typ → position dans .knot
        // ...
    }

    fn map_position_knot_to_typ(&self, knot_pos: Position) -> Position {
        // Convertir position dans .knot → position dans .typ
        // ...
    }
}
```

### Workflow complet avec mapping

```
1. Requête LSP du client sur .knot (line 7, col 15)
2. Map position knot → typ
3. Forward à tinymist avec position mappée
4. Recevoir réponse de tinymist
5. Map positions dans réponse typ → knot
6. Ajouter diagnostics knot (déjà en positions knot)
7. Renvoyer au client
```

---

## Modules de knot-lsp

### Structure proposée

```
crates/knot-lsp/
├── Cargo.toml
└── src/
    ├── main.rs              # Point d'entrée, setup serveur
    ├── server.rs            # Struct KnotLanguageServer, implémentation LanguageServer trait
    ├── proxy.rs             # Communication avec subprocess tinymist
    ├── transform.rs         # Transformation .knot → .typ factice
    ├── position_mapper.rs   # Mapping positions knot ↔ typ
    ├── diagnostics.rs       # Diagnostics spécifiques knot (chunks R)
    ├── symbols.rs           # Document symbols (outline chunks)
    ├── hover.rs             # Hover information pour chunks
    ├── completion.rs        # Completion options chunks
    └── compiler_bridge.rs   # Interface avec knot compile (background)
```

### Responsabilités des modules

#### `proxy.rs` - Communication avec tinymist
```rust
pub struct TinymistProxy {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl TinymistProxy {
    pub async fn spawn() -> Result<Self>;
    pub async fn send_request(&mut self, method: &str, params: Value) -> Result<Value>;
    pub async fn send_notification(&mut self, method: &str, params: Value) -> Result<()>;
    pub async fn shutdown(&mut self) -> Result<()>;
}
```

#### `transform.rs` - Transformation rapide
```rust
pub fn transform_to_placeholder(knot_content: &str) -> String;
pub fn transform_with_mapping(knot_content: &str) -> (String, PositionMapper);
```

#### `compiler_bridge.rs` - Compilation background
```rust
pub struct CompilerBridge {
    debouncer: Debouncer,
    cache: Arc<RwLock<Cache>>,
}

impl CompilerBridge {
    pub async fn compile_in_background(&self, knot_path: &Path) -> Result<PathBuf>;
    pub fn is_compiling(&self) -> bool;
    pub fn get_compilation_progress(&self) -> Option<CompilationProgress>;
}

pub struct CompilationProgress {
    pub current_chunk: usize,
    pub total_chunks: usize,
    pub current_chunk_name: Option<String>,
}
```

---

## Avantages de cette architecture

### Pour l'utilisateur
- ✅ **Expérience native Typst** : Tout le support Typst (hover, completion, preview)
- ✅ **Support R intégré** : Diagnostics chunks, outline, navigation
- ✅ **Preview intelligent** : Structure immédiate, résultats progressifs
- ✅ **Performance** : Cache évite recompilations inutiles
- ✅ **Un seul outil** : Pas besoin de jongler entre plusieurs extensions

### Pour le développement
- ✅ **Code minimal** : On ne réimplémente pas Typst
- ✅ **Maintenance légère** : tinymist suit les évolutions Typst
- ✅ **Focus sur knot** : On code uniquement les features R/chunks
- ✅ **Testabilité** : Modules séparés, transformation pure
- ✅ **Évolutivité** : Facile d'ajouter Python, Lilypond plus tard

### Pour les performances
- ⚡ **Preview structure** : < 100ms (transformation simple)
- 🚀 **Compilation intelligente** : Cache évite 90% des recompilations
- 🔄 **Asynchrone** : Preview immédiat + compilation background
- 📊 **Progressif** : Preview se met à jour chunk par chunk

---

## Défis techniques et solutions

### Défi 1 : Communication LSP-sur-LSP
**Problème** : knot-lsp doit parler LSP avec VS Code ET avec tinymist
**Solution** : Utiliser tower-lsp pour les deux côtés, JSON-RPC standard

### Défi 2 : Mapping des positions
**Problème** : Positions changent entre .knot et .typ factice
**Solution** : PositionMapper construit à chaque transformation, mapping bidirectionnel

### Défi 3 : Gestion du subprocess tinymist
**Problème** : tinymist peut crasher, être lent, etc.
**Solution** :
- Timeout sur les requêtes
- Retry avec backoff exponentiel
- Fallback : si tinymist down, continuer avec diagnostics knot uniquement

### Défi 4 : Compilation lente (code R)
**Problème** : Compilation peut prendre plusieurs minutes
**Solution** :
- Preview structure immédiate (< 100ms)
- Compilation background avec debouncing
- Cache agressif (Phase 3)
- Indicateurs de progression

### Défi 5 : Synchronisation état
**Problème** : Garder .knot, .typ factice, .typ compilé synchronisés
**Solution** :
- Version tracking (incremental updates)
- Source of truth : .knot dans knot-lsp
- .typ factice recalculé à la demande
- .typ compilé en background uniquement

---

## Métriques de succès

### Performance
- ⚡ Preview structure : < 100ms après frappe
- 🚀 Diagnostics : < 200ms après frappe
- 🔄 Compilation (cache hit) : < 2s
- 📊 Mémoire : < 200MB (serveur + tinymist)

### Fonctionnalité
- ✅ Tous les diagnostics Typst fonctionnent
- ✅ Tous les diagnostics knot fonctionnent
- ✅ Preview en temps réel
- ✅ Pas de faux positifs dans diagnostics
- ✅ Navigation fluide (symbols, jump to def)

### Expérience utilisateur
- ✅ Installation en un clic (extension VS Code)
- ✅ Pas de configuration manuelle
- ✅ Feedback immédiat sur les erreurs
- ✅ Preview toujours à jour
- ✅ Pas de lag perceptible

---

## Références

### Documentation
- [Tinymist GitHub](https://github.com/Myriad-Dreamin/tinymist)
- [Tinymist LSP Architecture](https://myriad-dreamin.github.io/tinymist/module/lsp.html)
- [tower-lsp Documentation](https://docs.rs/tower-lsp)
- [LSP Specification](https://microsoft.github.io/language-server-protocol/)

### Inspiration
- [rust-analyzer](https://github.com/rust-lang/rust-analyzer) - Proxy macro expansion
- [vue-language-server](https://github.com/vuejs/language-tools) - Multi-file coordination
- [astro-language-server](https://github.com/withastro/language-tools) - Framework over base language

---

## Conclusion

Cette architecture de **proxy intelligent** est la meilleure approche car elle :

1. **Réutilise l'excellent travail** de tinymist pour Typst
2. **Ajoute uniquement** ce qui est spécifique à knot (chunks R, inline)
3. **Fournit une expérience native** avec preview en temps réel
4. **Optimise intelligemment** avec cache et compilation background
5. **Reste maintenable** avec des modules clairement séparés

L'utilisateur final a une **expérience fluide** où :
- La structure du document est toujours à jour (< 100ms)
- Les résultats R apparaissent progressivement (cache intelligent)
- Les diagnostics sont précis (Typst + knot combinés)
- La navigation est naturelle (outline, hover, jump to def)

C'est **ambitieux mais réaliste** car on s'appuie sur des briques solides (tinymist, tower-lsp, notre compilateur existant avec cache).

---

**Document créé le** : 2026-01-13
**Dernière mise à jour** : 2026-01-13
**Statut** : Architecture définitive, prêt pour implémentation

---

# PART 3: DEVELOPMENT LOG

(Originally `DEVLOG.md`)

# Knot Project - Development Log

This document tracks the major development steps and architectural decisions made for the Knot project.

## Session: 2026-01-09

**Summary:** This session focused on moving beyond the initial Phase 1 prototype. We significantly refactored the core architecture, implemented the full caching system from Phase 3, and prepared the ground for future features like the LSP.

### Key Accomplishments:

1.  **Advanced Templating Architecture (`#code-chunk`)**
    *   Replaced the basic Typst generation with a more powerful and flexible system.
    *   The compiler now generates a single `#code-chunk()` function call for each chunk.
    *   This call passes all relevant metadata (language, name, caption, options) as named arguments, giving template authors full control over layout and presentation.
    *   Demonstrated this flexibility by implementing a two-column layout.

2.  **Chunk Referencing System**
    *   Successfully implemented a system for cross-referencing code chunks using standard Typst syntax (`@<chunk-name>`).
    *   This was achieved by having the compiler wrap named chunks in a Typst `#figure` element with `kind: raw` and attaching the chunk name as a label. This makes chunks first-class, numberable elements in the document.

3.  **Phase 3: Chained Caching System**
    *   Fully implemented the chained caching system as described in `knot-project-reference.txt`.
    *   The cache correctly invalidates based on changes to chunk code, options, preceding chunks (chaining), and external file dependencies (`#| depends:`).
    *   Added a full suite of unit tests for the caching logic, covering chaining, dependencies, and options.
    *   Verified the system works with manual runs, showing `[executing]` on the first run and `[cached]` on the second.

4.  **Local Typst Package (`knot-typst-package`)**
    *   Created a dedicated directory and structure for a local Typst package.
    *   Moved all presentation logic (the `#code-chunk` function, `codly` configuration, etc.) into this package.
    *   The main `.knot` document now simply imports this package, making it much cleaner and promoting reusability.

5.  **Code Quality & Readability**
    *   Integrated the `typstyle` code formatter into the compilation pipeline.
    *   The intermediate `.typ` file generated by `knot` is now automatically formatted, making it highly readable for debugging and inspection.

6.  **LSP Readiness (Parser Improvements)**
    *   Enhanced the parser (`parser.rs`) to accurately calculate the line/column start and end positions for both the entire chunk (`range`) and the code within it (`code_range`).
    *   This replaces the placeholder "dummy" values and is a critical prerequisite for building Language Server Protocol (LSP) features in the future.
    *   Added unit tests to verify the correctness of the position calculations.

### Current Status

The project has successfully completed all objectives for **Phase 1** and **Phase 3**. The core architecture is now robust, extensible, and performant. The next logical steps would be to begin **Phase 2 (R Package)** for richer R object display or **Phase 4 (Graphics Support)**.

---

## Session: 2026-01-10

**Summary:** This session focused on a comprehensive code quality audit and fixing critical issues identified during analysis. The codebase underwent important refactoring to eliminate duplication, centralize configuration, and improve robustness.

### Code Quality Audit

Performed a complete analysis of the existing codebase, identifying strengths and areas for improvement:

**Strengths Identified:**
- Solid architecture with clear separation between `knot-core` and `knot-cli`
- Extensible `LanguageExecutor` trait for future language support
- Functional chained cache with comprehensive unit tests
- Persistent R process executor (superior to subprocess approach)
- LSP-ready position tracking in parser

**Critical Issues Found:**
1. Regex pattern duplicated in `parser.rs` and `codegen.rs` (desynchronization risk)
2. Cache directory inconsistently defined (`/tmp/.knot_cache` vs `.knot_cache`)
3. Template initialization using fragile relative paths
4. Regex recompiled on every function call (performance issue)

### Critical Fixes Implemented

1.  **Shared Regex Pattern with Lazy Initialization**
    *   Added `once_cell` dependency to workspace
    *   Created `pub static CHUNK_REGEX: Lazy<Regex>` in `lib.rs`
    *   Updated `parser.rs` and `codegen.rs` to use shared pattern
    *   **Result:** Single source of truth, compiled once at first use

2.  **Centralized Cache Directory Configuration**
    *   Implemented `get_cache_dir()` function in `lib.rs`
    *   Replaced all hardcoded cache paths in `compiler.rs`
    *   **Result:** Cache now consistently located in `.knot_cache/` as per specification

3.  **Embedded Template in Binary**
    *   Used `include_str!("../../../templates/default.knot")` in `main.rs`
    *   Simplified `init()` function to write embedded template directly
    *   **Result:** `knot init` works from any directory, no external file dependency

4.  **Performance Optimization**
    *   Regex compilation reduced from O(n) to O(1) via lazy static
    *   Cache directory lookup centralized (eliminates redundant path constructions)

### Validation & Testing

All changes validated with comprehensive testing:
- ✅ `cargo build`: Clean compilation
- ✅ `cargo test`: All 8 unit tests passing (parser + cache)
- ✅ `knot init`: Template creation successful
- ✅ `knot compile`: PDF generation working correctly
- ✅ Cache verification: Second compilation shows `[cached]` on all chunks
- ✅ Cache location: Files correctly in `.knot_cache/` directory

**Cache structure verified:**
```
.knot_cache/
├── metadata.json (708 bytes, 2 chunks tracked)
├── chunk_35867996...bbe6.txt
└── chunk_c3c40d61...019a.txt
```

### Code Quality Improvements

| Metric | Before | After |
|--------|--------|-------|
| Regex definitions | 2 (duplicated) | 1 (shared, lazy) |
| Cache directory | Inconsistent (`/tmp/`) | Consistent (`.knot_cache/`) |
| Template loading | File I/O required | Embedded in binary |
| Regex compilations | Per function call | Once (lazy static) |

### Current Status

All **Phase 1 critical issues** have been resolved. The project is now more robust, maintainable, and performant. The project is ready to proceed with **Phase 2 (R Package)** or **Phase 4 (Graphics Support)** with a solid foundation.

---

## Session: 2026-01-11

**Summary:** This session successfully implemented **Phase 2 (R Package)** for rich output support. The focus was on enabling R dataframes to be rendered as properly formatted tables in Typst documents, with full caching and serialization support.

### Key Accomplishments:

1.  **ExecutionResult Extension**
    *   Added `DataFrame(PathBuf)` variant to `ExecutionResult` enum
    *   Enables type-safe handling of CSV-serialized dataframes
    *   Integrated with existing cache system for DataFrame results

2.  **R Package Development (`knot.r.package`)**
    *   Created S3 generic method `to_typst()` for extensible object serialization
    *   Implemented `to_typst.data.frame()` that serializes dataframes to CSV with `__KNOT_SERIALIZED_CSV__` marker
    *   Properly exported S3 methods in NAMESPACE
    *   Package automatically loaded by R executor on initialization

3.  **Output Parsing and Serialization**
    *   Implemented `RExecutor::parse_output()` to detect `__KNOT_SERIALIZED_CSV__` markers in R stdout
    *   CSV content extracted and saved to cache with SHA256-based filenames
    *   Automatic fallback to text output when no markers detected

4.  **Typst Table Generation**
    *   Compiler generates correct Typst syntax for multi-column tables:
      ```typst
      #{ 
        let data = csv("_knot_files/dataframe.csv")
        table(columns: data.first().len(), ..data.flatten())
      }
      ```
    *   Automatic column count detection via `data.first().len()`
    *   Single CSV read for performance

5.  **File Management Strategy**
    *   CSV files copied from `.knot_cache/` to `_knot_files/` directory alongside `.typ` file
    *   Post-processing step in CLI rewrites absolute paths to relative `_knot_files/` paths
    *   Enables Typst compilation with `--root .` while maintaining portability
    *   Similar to knitr/RMarkdown `*_files/` directory pattern

6.  **Cache Integration**
    *   Extended `Cache::save_result()` to handle DataFrame variant
    *   Extended `Cache::get_cached_result()` to restore CSV files from cache
    *   DataFrame results participate in chained cache invalidation

### Technical Implementation Details

**R Package Installation:**
```bash
cd knot-r-package && R CMD INSTALL .
```

**Package automatically loaded on R executor initialization:**
```rust
impl RExecutor {
    fn load_knot_package(&mut self) -> Result<()> {
        // Attempts library(knot.r.package)
        // Warns but doesn't fail if package not available
    }
}
```

**CSV Marker Protocol:**
- R package writes: `__KNOT_SERIALIZED_CSV__\n<CSV content>`
- Rust parser detects marker and extracts CSV
- CSV saved with hash-based filename: `dataframe_{hash}.csv`

**Path Resolution:**
- Compiler generates absolute paths initially
- CLI post-processes `.typ` file to copy CSVs and fix paths
- Regex pattern: `"(/[^"].+\.knot_cache/[^"].+)"` → `"_knot_files/{filename}"`

### Validation & Testing

All functionality verified with comprehensive example:

**Test Document:** `examples/phase2_dataframes/test_dataframes.knot`
- ✅ Simple 3x3 dataframe rendered as table
- ✅ Iris dataset (6x5) rendered as table
- ✅ Mixed output (text via `summary()`)
- ✅ Cache working correctly (`[cached]` on second run)
- ✅ All 8 unit tests still passing

**Generated Output:**
```
examples/phase2_dataframes/
├── test_dataframes.knot   # Source document
├── test_dataframes.typ    # Generated Typst (1.2KB)
├── test_dataframes.pdf    # Final PDF (57KB)
└── _knot_files/           # Local CSV files
    ├── dataframe_2090f3115d8f5e48.csv (68 bytes)
    └── dataframe_da383c2d8de04099.csv (213 bytes)
```

### Code Quality Metrics

| Component | Lines Added | Files Modified | Tests | 
|-----------|-------------|----------------|-------|
| knot-core | ~100 | 4 | 0 new (8 existing pass) |
| knot-cli | ~50 | 2 | - |
| R package | ~20 | 2 | - |
| **Total** | **~170** | **8** | **8/8 ✅** |

**Dependencies Added:**
- `pathdiff = "0.2"` (for relative path calculation)

### Current Status

**Phase 2 (R Package)** is now complete and production-ready. The system successfully:
- ✅ Renders R dataframes as formatted Typst tables
- ✅ Maintains cache efficiency (DataFrame results cached)
- ✅ Follows established patterns (similar to knitr/RMarkdown)
- ✅ Handles mixed output (text + dataframes)

**Next Steps:**
- **Phase 4 (Graphics Support)** - Add ggplot2 and base R plot support
- **Extend Phase 2** - Add `typst()` methods for vectors, matrices, model objects
- **Testing** - Add integration tests for end-to-end compilation

---

## Session: 2026-01-12

**Summary:** This session successfully implemented **Phase 4 (Graphics Support)** using the explicit approach (Approach B). We renamed `to_typst()` → `typst()` for clarity and added full ggplot2 plot support with caching and integration.

### Key Accomplishments:

1.  **Function Renaming: `to_typst()` → `typst()`**
    *   Updated all R package code to use the shorter, clearer `typst()` name
    *   Modified NAMESPACE to export `typst` and S3 methods
    *   Updated DESCRIPTION to include ggplot2 and digest as Suggests dependencies
    *   More idiomatic and concise for users

2.  **Plot Support via `typst.ggplot()` Method**
    *   Implemented S3 method for ggplot objects
    *   Saves plots using `ggplot2::ggsave()` with configurable options:
      - `width`, `height` (in inches)
      - `dpi` (resolution)
      - `format` (svg, png, pdf)
    *   Generates unique filenames based on plot content hash (SHA256)
    *   Outputs `__KNOT_SERIALIZED_PLOT__` marker with file path

3.  **Plot Detection in RExecutor**
    *   Extended `parse_output()` to detect `__KNOT_SERIALIZED_PLOT__` marker
    *   Handles multiple markers in single output (CSV + Plot)
    *   Copies plots from temp directory to `.knot_cache/`
    *   New `ExecutionResult` variants:
      - `Plot(PathBuf)` - standalone plot
      - `DataFrameAndPlot { dataframe, plot }` - combined output
    *   Renamed `Both` → `TextAndPlot` for clarity

4.  **Compiler Integration**
    *   Updated `compiler.rs` to generate `#image()` calls for plots
    *   Uses absolute paths with `canonicalize()` for consistency
    *   CLI copies plots to `_knot_files/` and fixes paths (same as CSVs)
    *   Handles all ExecutionResult variants (Plot, DataFrameAndPlot, TextAndPlot)

5.  **Improved Error Handling**
    *   Modified `r.rs` to distinguish real errors from R warnings/messages
    *   Pattern matching for actual errors: "Error", "Erreur", "Execution arrêtée"
    *   Logs warnings without failing (e.g., `geom_smooth()` messages)
    *   More robust execution flow

6.  **Cache Integration**
    *   Plot results fully integrated with SHA256-based cache system
    *   Cache invalidation works automatically when code changes
    *   Tested: changing plot dimensions generates new cached plot
    *   Metadata tracks plot files alongside DataFrames

### Implementation Details

**Approach B - Explicit Control:**
Instead of automatic R device wrapping (Approach A), we chose explicit `typst()` calls:

```r
# User code (explicit and predictable)
gg <- ggplot(iris, aes(x, y)) + geom_point()
typst(gg, width = 8, height = 5, dpi = 300)
```

**Benefits:**
- Simple, predictable, no "magic"
- Consistent with DataFrame approach (`typst(df)`)
- Full user control over output
- Easy to implement and maintain (~2-3h vs 6-7h for auto-wrapping)
- Options passed as function arguments (not chunk options)

**R Package Structure:**
```r
typst <- function(x, ...) UseMethod("typst")
typst.data.frame <- function(x, ...) { ... }  # Existing
typst.ggplot <- function(x, width=7, height=5, dpi=300, format="svg", ...) {
  ggsave(...); cat("__KNOT_SERIALIZED_PLOT__\n", filepath)
}
```

**Marker Protocol:**
- `__KNOT_SERIALIZED_CSV__` for DataFrames
- `__KNOT_SERIALIZED_PLOT__` for plots
- Both markers can appear in same output → `DataFrameAndPlot`

### Validation & Testing

Created comprehensive test suite in `examples/phase4_plots/`:

1.  **test_plots_phase4.knot** - Main test document:
    - ✅ Simple ggplot2 scatter plot
    - ✅ Plot with custom dimensions
    - ✅ Combined DataFrame + Plot output
    - ✅ Multiple plots in sequence

2.  **test_cache.knot** - Cache validation:
    - ✅ Initial plot generation
    - ✅ Cache invalidation on code change
    - ✅ New plot generated with different dimensions

**Test Results:**
- ✅ PDF generation successful (103 KB)
- ✅ Plots copied to `_knot_files/` (3 SVG files: 12K, 19K, 22K)
- ✅ Cache metadata correctly tracks plot files
- ✅ All existing tests still pass (Phase 1-3)

### Code Quality Metrics

| Component | Lines Added | Files Modified | Dependencies Added |
|-----------|-------------|----------------|-------------------|
| knot-core (r.rs) | ~200 | 1 | - |
| knot-core (mod.rs) | ~2 | 1 | - |
| knot-core (compiler.rs) | ~20 | 1 | - |
| knot-core (cache.rs) | ~15 | 1 | - |
| R package | ~50 | 3 | digest, ggplot2 (Suggests), svglite |
| Examples | ~100 | 3 new files | - |
| **Total** | **~387** | **10** | **3 packages** |

**Dependencies Added:**
- R packages: `digest` (for hashing), `ggplot2` (Suggests), `svglite` (for SVG export)

### Technical Decisions

**Why Approach B (Explicit) over Approach A (Auto-wrapping)?**

| Criteria | Auto (A) | Explicit (B) | 
|----------|----------|--------------|
| Implementation time | 6-7h | 2-3h ✅ |
| Code complexity | High | Low ✅ |
| User predictability | Magic | Explicit ✅ |
| Edge cases | Many | Few ✅ |
| Consistency | Different | Same as DataFrame ✅ |
| Maintenance | Complex | Simple ✅ |

**Decision:** Approach B aligns with knot's philosophy of explicit, predictable behavior.

### Current Status

**Phase 4 (Graphics Support)** is now complete and production-ready. The system successfully:
- ✅ Renders ggplot2 plots as images in Typst documents
- ✅ Supports SVG, PNG, and PDF formats
- ✅ Configurable dimensions and resolution
- ✅ Full cache integration (plots cached and invalidated correctly)
- ✅ Handles combined DataFrame + Plot output
- ✅ Explicit, user-controlled approach
- ✅ Follows established patterns (similar to DataFrame support)

**Completed Phases:**
- ✅ Phase 1: R execution with parser, executors, compiler
- ✅ Phase 2: R package with DataFrame → Typst table support
- ✅ Phase 3: SHA256-based cache with chained invalidation
- ✅ Phase 4: ggplot2 plot support with explicit `typst()` calls

**Next Steps:**
- **Phase 4B (Optional)** - Add base R plot support via `capture_plot()` helper
- **Phase 4C (Future)** - Native Typst graphics via CeTZ (if community tools emerge)
- **Phase 5** - Typst package publication to @preview
- **Phase 6** - Inline expressions, watch mode, global configuration
- **Testing** - Additional edge case coverage

---

## Session: 2026-01-12 (continued)

**Summary:** This session successfully implemented **Phase 6 (Inline Expressions)** with the `#r[expr]` syntax. After completing Phase 4 (Graphics), we added comprehensive testing infrastructure (README + 25 tests) and then implemented inline R evaluation for embedding computed values directly in text.

### Testing Infrastructure

**README Documentation:**
- Updated README to reflect Phases 1-4 completion
- Added installation instructions for R package
- Created "Rich Output with typst()" section with DataFrame and plot examples
- Enhanced caching documentation with management commands
- Updated roadmap with current status

**Integration Tests Suite:**
- Created `crates/knot-core/tests/integration_basic.rs` (5 tests):
  - Basic compilation, multiple chunks, anonymous chunks, empty documents, dependencies
- Created `crates/knot-core/tests/integration_execution.rs` (7 tests, #[ignore] by default):
  - R execution, error handling, session persistence, warnings vs errors
  - DataFrame serialization, plot generation, combined output
- Created `tests/README.md` documenting test structure and commands
- **Total test suite:** 25 tests (13 unit + 12 integration), all passing ✅

### Phase 6: Inline Expressions

**1. Parser Extension**
- Extended `Document` struct to include `inline_exprs: Vec<InlineExpr>` field
- Created `InlineExpr` struct with `language`, `code`, `start`, `end` fields
- Implemented `extract_inline_exprs()` to parse `#r[expr]` patterns from source
- Regex pattern: `# (r|python|lilypond)\[([^\\\]]+)\]`
- Correctly skips inline expressions inside code chunks
- Exported `InlineExpr` in public API (`lib.rs`)

**2. RExecutor Extension**
- Implemented `execute_inline(&mut self, code: &str) -> Result<String>` method
- **Smart output formatting** based on R result type:
  - **Scalars:** `[1] 150` → `150` (extract value, remove `[1]` prefix)
  - **Strings:** `[1] "Alice"` → `Alice` (remove quotes and prefix)
  - **Short vectors:** `[1] 1 2 3 4 5` → `` `[1] 1 2 3 4 5` `` (wrap in backticks)
- Helper functions:
  - `extract_scalar_value()`: Detects single-value R output and extracts cleanly
  - `is_short_vector_output()`: Identifies multi-value vectors (<80 chars)
- **Critical fix:** Correctly handles R's behavior where even scalars display with `[1]` prefix
- Rejects complex outputs (DataFrames, Plots) with descriptive error messages

**3. Compiler Integration**
- Implemented `find_inline_expressions()` helper with **proper bracket nesting**:
  - Handles expressions like `#r[letters[1:3]]` correctly
  - Manual bracket depth tracking instead of regex (which fails on nested `[]`)
  - Processes matches in reverse order to preserve byte offsets during replacement
- Extended `Compiler::compile()` to process inline expressions after chunk replacement:
  - Finds all `#r[...]` patterns in generated Typst output
  - Executes each via `RExecutor::execute_inline()`
  - Replaces pattern with formatted result
  - Comprehensive error messages with expression context

**4. Typst Syntax Understanding**
Important clarification on Typst inline code syntax:
- `` `text` `` - inline monospace, **no coloration**
- `` ```lang code``` `` - inline monospace, **with language coloration**
- Block format - separate block, with coloration

**Decision:** Use plain backticks for vectors (`` `[1] 1 2 3` ``) since R output is not valid R code.

### Implementation Example

**Source `.knot`:**
```typst
```{r}
x <- 150
df <- data.frame(a=1:3, b=4:6)
```

The variable is #r[x] and the dataframe has #r[nrow(df)] rows.
Vector: #r[1:5]
```

**Generated `.typ`:**
```typst
The variable is 150 and the dataframe has 3 rows.
Vector: `[1] 1 2 3 4 5`
```

### Technical Challenges & Solutions

**Challenge 1: R Scalar Output Format**
- **Issue:** Incorrectly assumed R scalars don't have `[1]` prefix
- **User insight:** Even `x <- 150; x` outputs `[1] 150` in R
- **Solution:** Rewrote `extract_scalar_value()` to parse `[1]` prefix and extract value

**Challenge 2: Nested Brackets**
- **Issue:** Regex `#r\[([^\\\]]+)\]` failed on `#r[letters[1:3]]`
- **Solution:** Implemented manual bracket depth tracking in `find_inline_expressions()`

**Challenge 3: Byte Offset Invalidation**
- **Issue:** Parser byte offsets invalid after chunk replacement in compiler
- **Solution:** Re-scan generated `.typ` output with regex instead of using parser offsets

**Challenge 4: Borrow Checker**
- **Issue:** Cannot iterate captures while mutating string
- **Solution:** Collect all match data `(lang, code, start, end)` before mutation

### Validation & Testing

**Test Document:** `examples/inline_expressions/test_inline.knot`

**Test Cases:**
- ✅ Scalar variables: `#r[x]` → `150`
- ✅ Function calls: `#r[round(y, 2)]` → `3.14`
- ✅ String variables: `#r[name]` → `Alice` (quotes removed)
- ✅ Arithmetic: `#r[10 + 5]` → `15`
- ✅ Nested brackets: `#r[letters[1:3]]` → `` `[1] "a" "b" "c"` ``
- ✅ DataFrame queries: `#r[nrow(df)]` → `10`, `#r[ncol(df)]` → `2`
- ✅ Statistical functions: `#r[round(mean(df$value), 1)]` → `100.4`
- ✅ Logical values: `#r[5 > 3]` → `TRUE`
- ✅ Short vectors: `#r[1:5]` → `` `[1] 1 2 3 4 5` ``
- ✅ Sequences: `#r[seq(2,10,by=2)]` → `` `[1]  2  4  6  8 10` ``

**All test cases passed successfully!** Generated `.typ` file shows correct replacements with proper formatting.

### Code Quality Metrics

| Component | Lines Added | Files Modified | Tests Added |
|-----------|-------------|----------------|-------------|
| parser.rs | ~70 | 1 | 0 (parsing logic) |
| executors/r.rs | ~80 | 1 | 0 (inline execution) |
| executors/mod.rs | ~1 | 1 | 0 (#[derive(Debug)]) |
| compiler.rs | ~65 | 1 | 0 (compilation) |
| lib.rs | ~1 | 1 | 0 (export InlineExpr) |
| integration tests | ~170 | 2 new files | 12 tests |
| README.md | ~110 | 1 | - |
| Examples | ~60 | 1 new file | - |
| tests/README.md | ~58 | 1 new file | - |
| **Total** | **~615** | **10** | **12 tests** |

**No new dependencies required** - implementation uses existing `regex` crate.

### Current Status

**Phase 6 (Inline Expressions)** is now complete and production-ready. The system successfully:
- ✅ Parses `#r[expr]` patterns from source documents
- ✅ Executes inline R expressions in persistent session
- ✅ Formats output intelligently (scalars vs vectors)
- ✅ Handles nested brackets correctly
- ✅ Provides clear error messages for complex outputs
- ✅ Integrates seamlessly with existing chunk execution
- ✅ Variables from chunks available in inline expressions

**Completed Phases:**
- ✅ Phase 1: R execution with parser, executors, compiler
- ✅ Phase 2: R package with DataFrame → Typst table support
- ✅ Phase 3: SHA256-based cache with chained invalidation
- ✅ Phase 4: ggplot2 plot support with explicit `typst()` calls

**Next Steps:**
- **Phase 4B (Optional)** - Add base R plot support via `capture_plot()` helper
- **Phase 4C (Future)** - Native Typst graphics via CeTZ (if community tools emerge)
- **Phase 5** - Typst package publication to @preview
- **Phase 6B** - Watch mode, global configuration (YAML frontmatter)
- **Phase 7** - LSP implementation
- **Testing** - Additional edge case coverage

---

## Session: 2026-01-12 (Refactoring & Finalization)

**Summary:** This session involved a deep architectural discussion and a major refactoring of the compiler to correctly support inline expression state, caching, and side-effects. The initial implementation of inline expressions was completely overhauled to be more robust, correct, and maintainable.

### Design Discussion: Inline Expression Behavior

A critical design discussion was held regarding the behavior of inline expressions (`#r[...]`):

1.  **Read-Only vs. Side-Effects:** We considered making inline expressions purely "read-only" by technically preventing them from modifying the R environment's state.
    *   **Conclusion:** This was deemed infeasible to implement reliably and performantly.
2.  **Comparison with Quarto:** We analyzed Quarto's approach, which technically allows side-effects in inline code but discourages it in their documentation.
3.  **Final Design Decision:** We opted for a more explicit approach that improves on Quarto's model:
    *   **`#r[...]`:** For displaying values. The output is "smartly" formatted (scalars vs vectors) and it errors on complex results. This is the primary, recommended syntax.
    *   **`#r:run[...]`:** A new verb explicitly for executing code for its side-effects only. This produces no output in the document.

This design makes the user's intent clear, provides power when needed, and avoids ambiguity.

### Architectural Refactoring: Single-Pass Compiler

The most significant change was the refactoring of the `Compiler` to use a single-pass architecture.

**Previous Flawed Architecture:**
- The compiler first executed all code chunks.
- Then, it executed all inline expressions separately.
- **Result:** State changes from inline expressions were not visible to chunks, and inline expressions were not cached.

**New Single-Pass Architecture:**
1.  **Unified Node Model:** Introduced an `ExecutableNode` enum that unifies `Chunk`s and `InlineExpr`s.
2.  **Single, Sorted Pass:** The compiler now builds a single list of all executable nodes, sorts them by their position in the source document, and executes them in that order.
3.  **State Correction:** State is now correctly shared. A variable modified by an inline expression is correctly seen by the next chunk or inline expression.
4.  **Cache Integration for Inline Expressions:** Inline expressions are now fully integrated into the chained caching system. Modifying an inline expression correctly invalidates the cache for all subsequent nodes (chunks or inline).

### Compiler Module Split

To facilitate the refactoring and improve maintainability, the monolithic `compiler.rs` file was split into a dedicated `compiler` module:
- `compiler/mod.rs`: Contains the main `Compiler` struct and the simplified single-pass loop.
- `compiler/chunk_processor.rs`: Contains all logic for processing a `Chunk`.
- `compiler/inline_processor.rs`: Contains all logic for processing an `InlineExpr`.

### Critical Bug Fix: Deadlock in R Executor

During testing of the new features, a critical deadlock was discovered.
- **Symptom:** Tests involving `#r:run[...]` would hang indefinitely.
- **Root Cause:** A subtle bug in the new `execute_side_effect_only` function in `r.rs`. The `cat()` command sent to R was missing a newline character (`\n`), causing the `read_line()` call in the Rust host to block forever.
- **Solution:** Added the missing `\n` to the `cat()` command, resolving the deadlock.

### Validation & Testing

- All 25 existing unit and integration tests were re-verified and are passing.
- A new integration test, `test_inline_run_verb_and_cache_invalidation`, was added to specifically validate:
  - ✅ Correct execution of the `#r:run` verb (state is modified, no output is produced).
  - ✅ Correct cache invalidation (modifying an inline expression correctly triggers re-execution of a subsequent chunk).

### Current Status

The implementation of **Phase 6 (Inline Expressions)** and the surrounding compiler architecture is now complete, robust, and fully tested. The system correctly handles state, caching, and side-effects in a predictable, single-pass execution model. The codebase is significantly more maintainable.

**Completed Work:**
- ✅ Major compiler refactoring to a single-pass architecture.
- ✅ `r:run` verb for side-effect-only inline expressions.
- ✅ Full cache-chain integration for inline expressions.
- ✅ Critical deadlock bug fixed.
- ✅ Comprehensive tests for the new functionality.

The project is in an excellent state to proceed with further features.

---

## Session: 2026-01-13

**Summary:** This session marked the beginning of **Phase 7 (LSP Implementation)**. We established the core architecture for the Knot Language Server, focusing on a proxy-based approach to leverage the existing Typst ecosystem while adding R-specific capabilities.

### Key Accomplishments:

1.  **LSP Architecture: The "Smart Proxy" Strategy**
    *   Designed a proxy architecture where `knot-lsp` acts as a middleware between the editor (VS Code) and `tinymist` (the Typst LSP).
    *   **Why?** This avoids reinventing the wheel for Typst support (syntax, formatting, preview) while allowing us to inject R-specific features (chunk execution, diagnostics).
    *   Implemented `transform.rs` to generate "fake" `.typ` files (replacing R chunks with placeholders) for `tinymist` consumption.

2.  **Asynchronous Communication Infrastructure**
    *   Refactored the initial synchronous "Talkie-Walkie" communication model to a fully asynchronous "Telephone" model using `tokio`.
    *   Implemented a robust background task loop in `TinymistProxy` that continuously reads from the subprocess `stdout`.
    *   Used `oneshot` channels for request/response pairing and `mpsc` channels for spontaneous notifications (like diagnostics).
    *   This ensures the main LSP loop is never blocked, even when `tinymist` is busy compiling.

3.  **Phase 2 Features: Diagnostics & Mapping**
    *   Implemented `PositionMapper` to translate line numbers between the source `.knot` file and the generated `.typ` file.
    *   Successfully merged diagnostics:
        *   **R Diagnostics:** From `knot-core` parser (syntax errors in chunks).
        *   **Typst Diagnostics:** Forwarded from `tinymist` and re-mapped to correct source positions.
    *   Users now see errors from both languages in a unified interface.

4.  **Hover & Completion Support**
    *   Implemented context-aware **Hover**:
        *   Over R chunk: Displays metadata (language, options status).
        *   Over Typst content: Forwards to `tinymist` (e.g., function documentation).
    *   Implemented context-aware **Completion**:
        *   Inside R chunk options (`#|`): Suggests `eval`, `echo`, `cache`, etc.
        *   Inside Typst content: Forwards to `tinymist` (standard Typst auto-completion).

5.  **Refactoring & Modularization**
    *   Refactored `main.rs` (which had grown to >600 lines) into a clean, modular structure.
    *   Created `src/state.rs` for centralized server state management.
    *   Moved feature logic to `src/handlers/{hover,completion,formatting}.rs`.
    *   This improved code readability and maintainability significantly.

### Technical Implementation Details

**Architecture Diagram:**
```
VS Code <--> knot-lsp (Router)
                 |
                 +--> [Handler] Hover/Completion (R logic)
                 |
                 +--> [Proxy] Tinymist (Typst logic)
                        ^
                        |
                        v
                     tinymist subprocess
```

**Key Modules Created:**
- `proxy.rs`: Async process management.
- `position_mapper.rs`: Bidirectional coordinate translation.
- `handlers/`: Feature-specific logic.
- `state.rs`: Shared `Arc<RwLock<...>>` state.

### Current Status

The **LSP Foundation** is complete and stable.
- ✅ Async Proxy working perfectly.
- ✅ Diagnostics merging active.
- ✅ Hover and Completion fully functional for both languages.
- ✅ Codebase is clean and modular.

**Next Steps:**
- **VS Code Extension:** Finalize the client-side extension to bundle `knot-lsp`.
- **Phase 4 Integration:** Live preview updates via LSP.
- **Formatter:** Polish `Air` integration for R formatting.

---

## Session: 2026-01-31

**Summary:** Major refactoring session focused on robustness and syntax modernization. We replaced the regex-based parser with a proper parser combinator (winnow), introduced an abstraction layer for code generation (Backend), and updated the inline syntax to be more standard.

### Key Accomplishments:

1.  **Parser Rewrite (Winnow Migration)**
    *   Replaced the fragile Regex-based parser in `knot-core` with a robust parser using `winnow` (v0.7).
    *   This provides better error reporting, safer parsing of nested structures, and easier extensibility.
    *   The new parser correctly handles position tracking (line/column) which is crucial for LSP accuracy.

2.  **Syntax Modernization: Backticks for Inline Code**
    *   **Old Syntax:** `#r[code]` (Typst-style)
    *   **New Syntax:** `` `{r} code` `` (Markdown/Pandoc-style)
    *   **Rationale:** This syntax is more consistent with code chunks (` ```{r} `), avoids conflicts with Typst functions, and is more agnostic of the host language (LaTeX, Markdown, Typst).
    *   Updated `extract_inline_exprs_winnow` to parse this new format.

3.  **Backend Architecture**
    *   Introduced the `Backend` trait in `knot-core` to decouple the execution logic from the output format.
    *   Implemented `TypstBackend`.
    *   **Benefit:** This prepares the architecture to support other output formats in the future (e.g., LaTeX, HTML) without rewriting the core compiler.

4.  **LSP Transformation Fix (UTF-16 Aware)**
    *   Refactored `knot-lsp` to use the new core parser instead of maintaining its own regexes.
    *   Implemented "smart padding": when masking code for Tinymist (the Typst LSP), we now replace characters with spaces while **preserving UTF-16 code unit counts**.
    *   **Result:** Exact cursor synchronization in VS Code, even with multi-byte characters or Emojis in the source code.
    *   Simplification of `PositionMapper` to a near-identity function.

5.  **VS Code Extension Update**
    *   Updated `knot.tmLanguage.json` to support syntax highlighting for the new inline backtick syntax.

### Technical Details

**New Inline Syntax:**
- Evaluation: `` `{r} 1+1` `` -> Replaced by result `2`
- Evaluation with options: `` `{r, echo=TRUE} x` `` (Future support)
- Raw code (ignored by Knot): `` `r x` `` -> Left as is (monospaced in Typst)

**Padding Strategy:**
Instead of removing chunks (which shifts lines), we replace content with spaces/newlines.
- `\n` is preserved -> Line numbers match.
- `char` is replaced by `N` spaces where `N = char.len_utf16()` -> Column numbers match in VS Code.

### Current Status

The project core is now much more solid.
- ✅ Parser is robust and tested.
- ✅ Syntax is cleaner and standard.
- ✅ LSP is perfectly synced.
- ✅ Architecture supports multiple backends.

**Next Steps:**
- **Python Support:** Implement `PythonExecutor`.
- **Backend expansion:** Prototype a LaTeX backend?


```