# TODO — Faiblesses de conception identifiées

> Analyse réalisée le 2026-02-15. Chaque item inclut le fichier, la ligne, et une description du problème.

---

## 🔴 Critique

### [C1] Cache `TextAndPlot` / `DataFrameAndPlot` non restauré depuis le cache
**Fichier** : `crates/knot-core/src/cache/storage.rs:96-118`

`get_cached_result` ne lit que `entry.files[0]` pour reconstruire le résultat, quelle que soit la variante réelle. Le commentaire l'admet : *"For now, we handle single file results"*. Quand un chunk produit `TextAndPlot` ou `DataFrameAndPlot` (deux fichiers en cache), la restauration ne renvoie que le premier fichier — le texte ou le dataframe est silencieusement perdu.

**Fix** : Reconstruire la variante correcte en inspectant le nombre et les extensions des fichiers dans `entry.files`.

---

### [C2] Risque de panique par slice trop courte (`hash[..8]`)
**Fichier** : `crates/knot-core/src/compiler/snapshot_manager.rs:56, 63, 139, 158`

```rust
&previous_hash[..8]   // panique si previous_hash < 8 octets
&node_hash[..8]       // idem
```

La garde `if previous_hash.is_empty()` (ligne 33) ne protège que contre la chaîne vide. Tout hash de moins de 8 caractères (JSON de cache corrompu, bug de génération) fait paniquer le thread de compilation.

**Fix** : Remplacer par `&previous_hash[..previous_hash.len().min(8)]` ou une fonction utilitaire `short_hash(h: &str) -> &str`.

---

### [C3] Aucun timeout sur les processus R/Python — risque de blocage infini
**Fichier** : `crates/knot-core/src/executors/r/process.rs:82-138`

`read_until_boundary()` spawne deux threads qui bloquent indéfiniment sur `read_line()`. Si le code utilisateur entre dans une boucle infinie, ou si R/Python crashe sans émettre le marqueur `---KNOT_CHUNK_BOUNDARY---`, la compilation se fige sans jamais se terminer ni propager d'erreur.

**Fix** : Implémenter un timeout sur `read_until_boundary()`. Options :
- Passer à des channels avec `Receiver::recv_timeout()`
- Utiliser `std::thread::spawn` + `JoinHandle::join()` avec un select sur un channel de timeout
- Envisager un `child.wait_timeout()` pour détecter les crashs du processus

---

## 🟠 Perte de données silencieuse

### [D1] Plots/dataframes multiples silencieusement ignorés
**Fichier** : `crates/knot-core/src/executors/mod.rs:106-121`

```rust
OutputMetadata::Plot { path, .. } => {
    plot_path = Some(path); // écrase le précédent à chaque itération
}
```

Si un chunk produit plusieurs graphiques (boucle `for` Python, multiple `print()` en R), tous sauf le dernier sont perdus. Idem pour les dataframes.

**Fix** : Décider d'une sémantique (prendre le dernier, prendre tous, erreur) et implémenter `ExecutionResult::MultiPlot(Vec<PathBuf>)` si nécessaire.

---

### [D2] `StringOrVec::as_str()` ne renvoie que le premier élément
**Fichier** : `crates/knot-core/src/executors/side_channel.rs:30-41`

```rust
StringOrVec::Vec(v) => {
    if v.is_empty() { "" } else { &v[0] } // éléments [1..] ignorés
}
```

`Display` concatène avec `\n`, mais `as_str()` renvoie seulement `v[0]`. Les appelants qui utilisent `as_str()` sur un `Vec` multi-lignes perdent silencieusement les lignes suivantes. Incohérence entre `Display` et `as_str()`.

**Fix** : Supprimer `as_str()` ou documenter explicitement la limitation. Si l'intent est "première ligne seulement", le documenter. Sinon, aligner avec `Display`.

---

### [D3] Options mal orthographiées dans `knot.toml` ingérées sans erreur
**Fichier** : `crates/knot-core/src/parser/ast.rs:211-212`

```rust
#[serde(flatten)]
pub other: HashMap<String, toml::Value>,
```

Ce champ capture toutes les clés inconnues. Une faute de frappe (`fig-widht`) est silencieusement stockée dans `other` sans avertissement. Seules les clés `codly-*` sont extraites ; toutes les autres sont ignorées.

**Fix** : Après `extract_codly_options()`, logger un avertissement pour chaque clé dans `other` qui ne commence pas par `codly-`.

---

## 🟡 Performance

### [P1] Cache en O(n) — devrait utiliser `HashMap`
**Fichier** : `crates/knot-core/src/cache/mod.rs:71-87, 97-101, 128-131, 139-141, 178-183`

Toutes les recherches dans `metadata.chunks` et `metadata.inline_expressions` sont des scans linéaires (`iter().find()`, `iter().any()`). Pour un document avec *n* chunks, une compilation complète fait O(n²) comparaisons.

```rust
self.metadata.chunks.iter().any(|entry| entry.hash == hash)  // O(n)
```

**Fix** : Remplacer `Vec<ChunkCacheEntry>` par `HashMap<String, ChunkCacheEntry>` et `Vec<InlineCacheEntry>` par `HashMap<String, InlineCacheEntry>`, indexés par hash. Adapter `CacheMetadata` et la sérialisation JSON.

---

### [P2] Écriture de métadonnées sur disque à chaque chunk individuel
**Fichier** : `crates/knot-core/src/cache/mod.rs:103, 134, 186`

`save_metadata()` est appelé à chaque `save_inline_result()` et `save_result()`, soit une fois par chunk/expression inline. Pour un document de 50 chunks, 50 écritures atomiques (temp file + rename) sur disque.

**Fix** : Marquer le cache comme "dirty" et ne sauvegarder qu'une fois en fin de compilation (dans `Compiler::compile()`, après la boucle principale). `Cache::save_metadata()` est déjà public et appelé à la fin de `compile()` — supprimer les appels intermédiaires dans `save_result` et `save_inline_result`.

---

### [P3] `Cache` recréé depuis le disque à chaque sauvegarde LSP
**Fichier** : `crates/knot-lsp/src/main.rs:354-368`

`sync_with_cache()` est appelé sur chaque `did_save` et relit `metadata.json` depuis le disque via `Cache::new()`. Pour un projet actif, c'est une lecture disque inutile à chaque frappe + sauvegarde.

**Fix** : Conserver une instance `Cache` par document ouvert dans `ServerState` (ou `DocumentState`), mise à jour uniquement si le fichier `metadata.json` a changé depuis la dernière lecture (comparer mtime).

---

## 🔵 Design

### [De1] `start_byte` utilisé comme identifiant de chunk dans le cache (instable)
**Fichier** : `crates/knot-core/src/compiler/chunk_processor.rs:100, 111`

```rust
cache.save_error(
    chunk.start_byte,  // passé comme "chunk_index" — byte offset, pas index stable
    ...
)
```

Un byte offset change dès qu'on modifie le document avant ce chunk. Deux compilations successives sur un fichier édité produisent des entrées de cache avec des `index` différents pour le même chunk logique.

**Fix** : Numéroter les chunks lors du parsing (`chunk.index: usize` dans `Chunk`) ou utiliser le nom du chunk comme identifiant. L'`index` devrait être la position ordinale dans le document, pas un byte offset.

---

### [De2] `Document::parse()` retourne `Result` qui ne peut jamais échouer
**Fichier** : `crates/knot-core/src/parser/ast.rs:527-530`

```rust
pub fn parse(source: String) -> Result<Self> {
    let doc = super::winnow_parser::parse_document(&source);
    Ok(doc)  // toujours Ok — les erreurs de syntaxe vont dans doc.errors
}
```

Le type de retour `Result` est trompeur. Le parseur winnow est conçu pour toujours réussir (erreurs stockées dans `doc.errors`). Tous les appelants font `.unwrap()` ou `?` inutilement.

**Fix** : Changer la signature en `pub fn parse(source: String) -> Self` et adapter les appelants. Ou, si on veut conserver `Result` pour une future évolution, documenter explicitement que la valeur est toujours `Ok`.

---

### [De3] Type `Option<Option<u32>>` pour `digits` dans `InlineOptions`
**Fichier** : `crates/knot-core/src/parser/ast.rs:501-505`

```rust
define_inline_options! {
    digits: Option<u32> = None,
    // expand_type!(val, Option<u32>) => le champ stocké est Option<Option<u32>>
}
```

Définir une valeur nécessite `Some(Some(3))`. Ce double wrapping est source de confusion.

**Fix** : Introduire un kind `opt` dans le macro `define_inline_options!` (analogue au kind `opt` existant dans `define_options!`) pour que `digits` soit stocké comme `Option<Option<u32>>` mais avec une syntaxe de définition claire, ou reconsidérer si `Option<u32>` pur convient.

---

### [De4] Texte d'erreur en français codé en dur dans le compilateur Rust
**Fichier** : `crates/knot-core/src/compiler/mod.rs:188-205`

```rust
"=== Erreur d'exécution ({})\nDans le {} `{}`\n...\n_L'exécution des blocs `{}` suivants a été suspendue._"
```

Tout le reste du code (logs, erreurs, commentaires) est en anglais. Ce bloc d'erreur en français sera inintelligible pour les utilisateurs non-francophones. Il s'agit aussi de texte Typst généré dynamiquement, ce qui couple le format de sortie au code Rust.

**Fix** : Passer ces chaînes en anglais pour la cohérence du codebase. À terme, envisager de les déplacer dans le package Typst (`knot-typst-package`) pour qu'elles soient personnalisables par l'utilisateur.

---

### [De5] Trois endroits à modifier pour ajouter un nouveau langage
**Fichiers** :
- `crates/knot-core/src/defaults.rs:25` (`SUPPORTED_LANGUAGES`)
- `crates/knot-core/src/executors/manager.rs:66-78` (branche `match lang`)
- `crates/knot-core/src/config.rs:132-147` (`get_language_defaults`, `get_language_error_defaults`)

L'ajout de Julia ou d'un autre langage nécessite de modifier ces trois fichiers séparément, sans qu'aucun compilateur ne signale l'oubli d'un endroit.

**Fix** : Introduire un `enum Language { R, Python }` (ou un trait `LanguageDescriptor`) comme source de vérité unique. Utiliser `match` exhaustif dans les trois endroits pour bénéficier des erreurs de compilation lors de l'ajout d'un variant.

---

## 🟣 Qualité de code

### [Q1] Double lookup `HashMap` dans `get_executor`
**Fichier** : `crates/knot-core/src/executors/manager.rs:64-88`

```rust
if !self.executors.contains_key(lang) {   // lookup 1
    ...
    self.executors.insert(lang.to_string(), executor);
}
match self.executors.get_mut(lang) {       // lookup 2
    Some(e) => Ok(e.as_mut()),
    None => anyhow::bail!("..."),         // branche impossible en pratique
}
```

Anti-pattern classique Rust. La branche `None` finale ne peut pas être atteinte logiquement mais existe quand même, masquant l'invariant.

**Fix** :
```rust
if !self.executors.contains_key(lang) {
    let executor = /* créer */;
    self.executors.insert(lang.to_string(), executor);
}
Ok(self.executors.get_mut(lang).expect("just inserted"))
```
Ou restructurer avec `entry()` si le borrow checker le permet.

---

### [Q2] Duplication : `format_codly_call` et `format_local_call`
**Fichier** : `crates/knot-core/src/backend.rs:6-21`

Corps identiques, seul le nom de la fonction Typst diffère.

```rust
// Ces deux fonctions sont identiques à un détail près :
format!("#codly({})", args.join(", "))
format!("#local({})", args.join(", "))
```

**Fix** :
```rust
fn format_typst_call(fn_name: &str, options: &HashMap<String, String>) -> String {
    let args: Vec<String> = options.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
    format!("#{}({})", fn_name, args.join(", "))
}
pub fn format_codly_call(options: &HashMap<String, String>) -> String { format_typst_call("codly", options) }
pub fn format_local_call(options: &HashMap<String, String>) -> String { format_typst_call("local", options) }
```

---

### [Q3] Duplication des helpers de test entre modules
**Fichiers** :
- `crates/knot-core/src/compiler/chunk_processor.rs:240-254`
- `crates/knot-core/src/compiler/inline_processor.rs:89-98`

`setup_test_cache()`, `setup_test_manager()`, `setup_test_config()`, `create_test_chunk()` sont quasi-identiques dans les deux fichiers.

**Fix** : Créer `crates/knot-core/src/compiler/test_utils.rs` marqué `#[cfg(test)]` avec ces helpers partagés.

---

### [Q4] Ordre de locks implicite dans le LSP — risque de deadlock futur
**Fichier** : `crates/knot-lsp/src/handlers/formatting.rs:93-105`

```rust
let mut tinymist_guard = state.tinymist.write().await;   // acquiert tinymist
{
    let mut docs = state.documents.write().await;         // acquiert docs PENDANT tinymist
    ...
}
```

Dans `forward_to_tinymist` (`main.rs:302-351`), l'ordre est : `documents.read()` → relâche → `tinymist.write()`. L'ordre n'est pas circulaire aujourd'hui, mais l'absence de documentation de l'ordre `(tinymist > documents)` rend un futur deadlock difficile à diagnostiquer.

**Fix** : Documenter l'ordre de verrouillage dans un commentaire au-dessus de `ServerState`. Convention à établir : toujours acquérir dans l'ordre `tinymist` → `documents` → `executors` → `loaded_snapshot_hash`.

---

### [Q5] `hash_dependencies` basé sur mtime — résolution insuffisante sur certains FS
**Fichier** : `crates/knot-core/src/cache/hashing.rs:79-93`

```rust
hasher.update(format!("{:?}", modified).as_bytes());  // mtime seulement
hasher.update(metadata.len().to_string().as_bytes()); // + taille
```

Sur macOS (HFS+) : résolution 1 seconde. Deux modifications rapides dans la même seconde ne sont pas détectées. Sur FAT32 : résolution 2 secondes.

**Fix** : Hacher également les N premiers octets du fichier (ex. 4 Ko), ou le fichier complet si petit. Cela rend la détection de changement insensible à la résolution du filesystem.

---

### [Q6] Asymétrie vérification d'intégrité entre R et Python pour les objets constants
**Fichier** :
- `crates/knot-core/src/executors/python/mod.rs:154-169` (vérifie le hash du fichier .pkl)
- `crates/knot-core/src/executors/r/mod.rs` (pas de vérification)

Python vérifie l'intégrité du fichier sérialisé avant de le charger. R ne fait pas cette vérification. Une corruption de cache est détectée côté Python mais silencieusement ignorée côté R.

**Fix** : Implémenter la même vérification dans `RExecutor::load_constant()`. La méthode `hash_file()` de `PythonExecutor` devrait être déplacée dans un module commun `executors/integrity.rs`.

---

### [Q7] `TypstBackend::new()` instancié à chaque chunk sans nécessité
**Fichier** : `crates/knot-core/src/compiler/chunk_processor.rs:135`

```rust
let backend = TypstBackend::new();  // struct vide, recréée à chaque chunk
```

`TypstBackend` est une struct sans état (`struct TypstBackend;`). L'instancier à chaque chunk est inutile.

**Fix** : Passer le backend en paramètre de `process_chunk` ou l'instancier une seule fois dans `Compiler::compile()`.

---

### [Q8] `Cache::get_chunk_hash` : indirection inutile vers une fonction de module
**Fichier** : `crates/knot-core/src/cache/mod.rs:43-58`

```rust
pub fn get_chunk_hash(&self, ...) -> String {
    hashing::get_chunk_hash(...)  // délégation directe, self n'est pas utilisé
}
```

La méthode ne consulte aucun champ de `self`. C'est une indirection pure vers la fonction de module.

**Fix** : Rendre `hashing::get_chunk_hash()` publique (ou `pub(crate)`) et l'appeler directement, ou justifier la méthode wrapper par un futur besoin de contexte.

---

### [Q9] Vérification des objets constants : O(n) round-trips réseau vers l'exécuteur
**Fichier** : `crates/knot-core/src/compiler/mod.rs:293-328`

La phase de vérification finale recharge (`load_constant`) et re-hashe (`hash_object`) chaque objet constant via l'exécuteur. Pour *k* objets constants, cela représente 2k round-trips REPL R/Python supplémentaires après la compilation.

**Fix** : Conserver les hashes initiaux en mémoire pendant la compilation (déjà fait : `constant_objects: HashMap`) et comparer directement aux hashes calculés lors de la déclaration, sans recharger les objets.

---

## 📋 Récapitulatif par fichier

| Fichier | Items |
|---------|-------|
| `cache/storage.rs` | C1 |
| `compiler/snapshot_manager.rs` | C2 |
| `executors/r/process.rs` | C3 |
| `executors/mod.rs` | D1 |
| `executors/side_channel.rs` | D2 |
| `parser/ast.rs` | D3, De2, De3 |
| `cache/mod.rs` | P1, P2, Q8 |
| `lsp/main.rs` | P3, Q4 |
| `compiler/mod.rs` | De4, Q9 |
| `compiler/chunk_processor.rs` | De1, Q7 |
| `defaults.rs` + `manager.rs` + `config.rs` | De5 |
| `backend.rs` | Q2 |
| `compiler/chunk_processor.rs` + `inline_processor.rs` | Q3 |
| `executors/manager.rs` | Q1 |
| `cache/hashing.rs` | Q5 |
| `executors/python/mod.rs` + `executors/r/mod.rs` | Q6 |
