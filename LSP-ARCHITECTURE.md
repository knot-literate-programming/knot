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
│  • Gère deux flux parallèles :                                  │
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
│  • Fournit :                                                     │
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

```{r setup}
library(tidyverse)
df <- read.csv("data.csv")
```

Le dataset contient #r[nrow(df)] observations.

```{r plot}
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

## Plan d'implémentation

### Phase 1 : Infrastructure du proxy ✅ (En cours)
- [x] Créer crate knot-lsp
- [x] Setup serveur LSP basique
- [x] Modules diagnostics.rs et symbols.rs
- [ ] Module proxy.rs (spawner tinymist)
- [ ] Module transform.rs (transformation factice)
- [ ] Module position_mapper.rs
- [ ] Tests de communication proxy ↔ tinymist

### Phase 2 : Intégration basique
- [ ] Forward textDocument/didOpen à tinymist
- [ ] Forward textDocument/didChange à tinymist
- [ ] Recevoir diagnostics de tinymist
- [ ] Mapper positions typ → knot
- [ ] Merger diagnostics (tinymist + knot)
- [ ] Tests avec VS Code

### Phase 3 : Fonctionnalités complètes
- [ ] Hover combiné (Typst + chunks R)
- [ ] Completion combinée
- [ ] Document symbols combiné
- [ ] Jump to definition
- [ ] Tests d'intégration

### Phase 4 : Preview intelligent
- [ ] Module compiler_bridge.rs
- [ ] Debouncing des compilations
- [ ] Notification de progression
- [ ] Update preview après compilation
- [ ] Indicateurs visuels dans preview
- [ ] Gestion des erreurs de compilation

### Phase 5 : Extension VS Code
- [ ] Créer extension knot-vscode
- [ ] Configuration LSP (.knot files → knot-lsp)
- [ ] Vérification présence tinymist
- [ ] Syntaxe highlighting .knot
- [ ] Snippets pour chunks
- [ ] Publication VS Code Marketplace

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
