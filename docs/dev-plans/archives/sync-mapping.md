# Plan: Sync Mapping entre .typ compilé et .knot source

**Date:** 2026-02-05
**Objectif:** Permettre le sync (forward/backward) entre PDF et source .knot en mappant les positions via des marqueurs de chunks dans le .typ compilé.

## 🔄 Real-time LSP Synchronization (LSP-Only)

Before the final PDF synchronization, Knot implements a real-time synchronization layer for the IDE (LSP).

### knot-virtual:// Scheme
To avoid conflicts with local files and provide immediate feedback, the Knot LSP uses a virtual URI scheme:
- **Source**: `file:///path/to/doc.knot`
- **Virtual Typst**: `knot-virtual:///path/to/doc.knot.typ`

**Characteristics:**
- ✅ **Atomic State**: The `DocumentState` in the LSP ensures text, version, and position mappers are always in sync.
- ✅ **Isolation**: Tinymist sees a pure Typst document, while Knot manages the underlying R/Python code.
- ✅ **Position Mapping**: A robust `PositionMapper` translates coordinates between the raw Knot source and the virtual Typst mask in real-time.

---

## 🎯 Problème à résoudre (PDF Sync)

## 💡 Solution proposée

### Approche : Marqueurs de chunks avec commentaires Typst

Entourer chaque chunk transformé dans le `.typ` compilé avec des commentaires spéciaux :

```typst
// #BEGIN-CHUNK chunk-1-abc123
#code-chunk(lang: "r", show: both, input: [...], output: [...])
// #END-CHUNK chunk-1-abc123
```

**Avantages :**
- ✅ Les commentaires Typst sont invisibles dans le PDF
- ✅ Parser avec winnow pour extraire les offsets
- ✅ Mapping bidirectionnel : position ↔ chunk ID ↔ position source
- ✅ Pas d'overlap entre chunks donc ID simple suffit
- ✅ Permettrait le sync avec tinymist preview !

---

## 🛠️ Plan d'implémentation

### Phase 1 : Ajouter les marqueurs dans le .typ généré

**Fichier à modifier :** `crates/knot-core/src/backend.rs`
**Fonction :** `TypstBackend::format_chunk()` (lignes 17-120)

**Modification proposée :**

```rust
impl Backend for TypstBackend {
    fn format_chunk(&self, chunk: &Chunk, resolved_options: &ResolvedChunkOptions,
                   result: &ExecutionResult) -> String {
        // ... code existant pour générer code_chunk_call ...

        // Générer un ID unique pour le chunk
        let chunk_id = chunk.name.as_deref()
            .map(|n| n.to_string())
            .unwrap_or_else(|| {
                // Pour chunks sans nom, utiliser position ou hash
                format!("chunk-{}", chunk.range.start.line)
                // Ou avec hash : format!("chunk-{:x}", xxhash64(chunk.code))
            });

        // Wrapper avec marqueurs de début et fin
        format!(
            "// #BEGIN-CHUNK {}\n{}\n// #END-CHUNK {}\n",
            chunk_id,
            code_chunk_call,
            chunk_id
        )
    }
}
```

**Pour les inline expressions :**
Décider si on les marque aussi (probablement oui pour un mapping complet) :

```rust
// Dans execution.rs
format!("// #BEGIN-INLINE {} {}\n// #END-INLINE {}\n",
        inline_id, formatted_result, inline_id)
```

---

### Phase 2 : Parser les marqueurs avec winnow

**Nouveau fichier :** `crates/knot-core/src/sync/position_mapper.rs`

**Structure de données :**

```rust
pub struct SyncMapper {
    /// Map chunk_id → (start_offset, end_offset) dans le .typ compilé
    chunk_positions: HashMap<String, (usize, usize)>,
    /// Référence au Document source pour retrouver les positions .knot
    source_chunks: HashMap<String, Range>,
}

impl SyncMapper {
    /// Parse le .typ compilé pour extraire les positions des chunks
    pub fn from_typ(typ_content: &str, knot_doc: &Document) -> Result<Self> {
        let chunk_positions = parse_chunk_markers(typ_content)?;
        let source_chunks = build_source_map(knot_doc);

        Ok(SyncMapper { chunk_positions, source_chunks })
    }

    /// Mapping : position dans .typ → position dans .knot
    pub fn typ_to_knot(&self, typ_pos: usize) -> Option<Position> {
        // 1. Trouver le chunk contenant cette position dans le .typ
        let chunk_id = self.find_chunk_at_position(typ_pos)?;

        // 2. Retrouver la position du chunk dans le .knot source
        let knot_range = self.source_chunks.get(&chunk_id)?;

        // 3. Mapper vers le début du chunk (ou proportionnellement)
        Some(knot_range.start)
    }

    /// Mapping inverse : position dans .knot → position dans .typ
    pub fn knot_to_typ(&self, knot_pos: Position) -> Option<usize> {
        // 1. Trouver le chunk contenant cette position dans le .knot
        let chunk_id = self.find_knot_chunk(knot_pos)?;

        // 2. Retrouver la position dans le .typ compilé
        let (start, _end) = self.chunk_positions.get(&chunk_id)?;
        Some(*start)
    }
}

/// Parser winnow pour extraire les marqueurs
fn parse_chunk_markers(input: &str) -> Result<HashMap<String, (usize, usize)>> {
    use winnow::prelude::*;

    let mut positions = HashMap::new();
    let mut offset = 0;

    // Parser pattern: // #BEGIN-CHUNK <id>
    // ... contenu ...
    // // #END-CHUNK <id>

    // TODO: Implémenter avec winnow

    Ok(positions)
}
```

---

### Phase 3 : Intégrer dans le workflow de compilation

**Fichier :** `crates/knot-cli/src/lib.rs` ou nouveau module

Après la compilation, créer et sauvegarder le `SyncMapper` :

```rust
pub fn compile_file(file: &PathBuf, output_path: Option<&PathBuf>) -> Result<PathBuf> {
    // ... compilation existante ...

    // Écrire le .typ avec les marqueurs
    fs::write(&typ_output_path, fixed_source)?;

    // Créer le SyncMapper
    let sync_mapper = SyncMapper::from_typ(&fixed_source, &doc)?;

    // Sauvegarder en JSON pour utilisation par LSP/preview
    let sync_path = typ_output_path.with_extension("sync.json");
    sync_mapper.save(&sync_path)?;

    Ok(typ_output_path)
}
```

---

### Phase 4 : Utiliser dans VS Code extension

**Fichier :** `editors/vscode/src/extension.ts`

**Nouveau module :** Intercepter les events de sync

```typescript
// Charger le mapping
const syncMapPath = path.join(projectRoot, '.main.sync.json');
const syncMapper = JSON.parse(fs.readFileSync(syncMapPath, 'utf-8'));

// Intercepter les clics dans le PDF preview
// (si on utilise tinymist ou un viewer custom)
previewServer.on('sync', (typPosition: number) => {
    // Mapper vers position .knot
    const knotPosition = mapTypToKnot(syncMapper, typPosition);

    // Ouvrir le fichier .knot à cette position
    const knotUri = Uri.file(knotPath);
    const pos = new Position(knotPosition.line, knotPosition.character);

    window.showTextDocument(knotUri, {
        selection: new Range(pos, pos),
        viewColumn: ViewColumn.One
    });
});
```

---

## 🔄 Flow complet du sync

### Forward sync (source → PDF)

1. Utilisateur clique dans le code .knot (ligne N)
2. Charger `SyncMapper` depuis `.main.sync.json`
3. Mapper position .knot → chunk_id → position .typ
4. Utiliser SyncTeX pour mapper position .typ → position PDF
5. Scroller le viewer PDF vers cette position

### Backward sync (PDF → source)

1. Utilisateur clique dans le PDF
2. SyncTeX donne position dans le .typ
3. Charger `SyncMapper` depuis `.main.sync.json`
4. Mapper position .typ → chunk_id → position .knot
5. Ouvrir le fichier .knot et scroller vers cette ligne

---

## 📋 Checklist d'implémentation

### Core (Rust)
- [ ] Modifier `backend.rs` pour ajouter marqueurs `#BEGIN-CHUNK` / `#END-CHUNK`
- [ ] Ajouter marqueurs pour inline expressions
- [ ] Créer module `sync/position_mapper.rs`
- [ ] Implémenter parser winnow pour extraire les marqueurs
- [ ] Implémenter mapping bidirectionnel typ ↔ knot
- [ ] Sauvegarder mapping en JSON après compilation
- [ ] Tests unitaires pour le parsing et mapping

### CLI
- [ ] Intégrer génération du `.sync.json` dans `compile_file()`
- [ ] Option CLI pour désactiver les marqueurs (si besoin)

### VS Code Extension
- [ ] Charger `.sync.json` dans l'extension
- [ ] Implémenter forward sync (Ctrl+Click dans .knot)
- [ ] Implémenter backward sync (Ctrl+Click dans PDF)
- [ ] Intégration avec tinymist preview (optionnel)
- [ ] Intégration avec PDF viewer externe via SyncTeX (optionnel)

### Documentation
- [ ] Documenter le format des marqueurs
- [ ] Documenter le format `.sync.json`
- [ ] Guide d'utilisation du sync dans VS Code

---

## 🚀 Extensions futures possibles

1. **Sync proportionnel :** Au lieu de mapper vers le début du chunk, calculer la position relative dans le chunk
2. **Multi-fichiers :** Support des projets avec plusieurs fichiers .knot
3. **Inline expressions :** Mapping fin des expressions inline
4. **Visualisation :** Overlay dans le PDF montrant les chunks sources
5. **Preview live :** Auto-scroll du PDF quand on édite le source

---

## 📝 Notes techniques

### Choix d'ID pour les chunks

**Option 1 : Position ligne (simple)**
```rust
format!("chunk-{}", chunk.range.start.line)
```
✅ Simple
❌ Instable si on ajoute/supprime des lignes avant

**Option 2 : Hash du contenu**
```rust
format!("chunk-{:x}", xxhash64(chunk.code))
```
✅ Stable même avec modifications avant
❌ Change si le code du chunk change

**Option 3 : Nom explicite (meilleur)**
```rust
chunk.name.as_deref().unwrap_or(&format!("unnamed-{}", index))
```
✅ Stable et lisible
✅ Fonctionne avec chunks nommés
✅ Index séquentiel pour chunks sans nom

**Recommandation :** Option 3 (nom + index séquentiel)

### Format des commentaires

Les commentaires Typst commencent par `//` et sont ignorés par le compilateur :

```typst
// Ceci est un commentaire
// #BEGIN-CHUNK mon-chunk
#code-chunk(...)
// #END-CHUNK mon-chunk
```

Pas d'impact sur :
- Le rendu PDF
- Les performances de compilation
- SyncTeX de base (Typst l'ignore dans son mapping)

---

## 🔗 Références

- Code de compilation : `crates/knot-core/src/compiler/mod.rs`
- Backend Typst : `crates/knot-core/src/backend.rs`
- Document structure : `crates/knot-core/src/document.rs`
- Winnow parsing : https://docs.rs/winnow/
- SyncTeX spec : http://www.tug.org/TUGboat/tb29-3/tb93laurens.pdf
