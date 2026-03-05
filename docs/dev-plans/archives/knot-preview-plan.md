# Plan : feat/knot-preview

## Vision

Trois commandes bien séparées, sans chevauchement de responsabilités :

| Commande | Sortie | Progressif | tinymist |
|---|---|---|---|
| `knot build` (CLI) | PDF one-shot | non | non |
| `knot watch` (CLI) | PDF auto-rebuild | non | non |
| Bouton "Start Preview" (VS Code / LSP) | Browser preview | **oui** | oui |

**Règle absolue :**
- Le CLI ne connaît pas tinymist et ne gère pas de browser.
- Le LSP ne génère pas de PDF.
- `knot-core` n'a pas d'opinion sur le mode de sortie.

---

## Ce qu'on garde — ne pas toucher

Tout le travail des étapes 0, 1, 2 dans `knot-core` est **solide et nécessaire** :

- `pipeline.rs` : `PlannedNode` owned, `ExecutionNeed` (Skip / CacheHit / MustExecute)
- `execution.rs` : parallélisme R/Python, `ProgressEvent`, `run_language_chain`
- `node_output.rs` : `pending_output`, `skip_output`
- `mod.rs` : `plan_and_partial`, `execute_and_assemble_streaming`, `assemble_pass`

Le code cassé est uniquement dans `knot-lsp/src/server_impl.rs` (implémentation de la preview).
Les fonctionnalités LSP existantes (proxy, diagnostics, completion, hover, formatting, sync) sont intactes.

---

## Phase 0 — Mise en ordre des branches

1. Merger `refactor/async-prereqs` (HEAD actuel = `61beec8`) dans `master`
2. Créer branche `feat/knot-preview` depuis `master`
3. Dans `server_impl.rs`, supprimer le code cassé :
   - `do_compile_streaming`
   - `assemble_project_typ`
   - `compile_knot_partial`
   - `write_and_notify_preview` (version actuelle)
4. Dans `state.rs`, supprimer `preview_typ_version: Arc<AtomicU64>` (remplacé par l'enum Phase 2)

---

## Phase 1 — API projet dans `knot-core`

Objectif : une seule implémentation de l'assemblage projet, partagée entre CLI et LSP.
Actuellement `knot-cli::build_project` et `server_impl.rs::assemble_project_typ` dupliquent cette logique.

### Nouveau fichier : `knot-core/src/project.rs`

```rust
/// Résultat d'une compilation projet
pub struct ProjectOutput {
    pub typ_content: String,
    pub main_typ_path: PathBuf,
    pub project_root: PathBuf,
}

/// Phase 0 : plan + assemble partiel (cache hits + placeholders).
/// Aucune exécution de chunks — instantané.
pub fn compile_project_phase0(start_path: &Path) -> Result<ProjectOutput>

/// Compilation complète avec streaming optionnel par chunk.
/// Si `progress` est fourni, un ProgressEvent est envoyé après chaque chunk exécuté.
pub fn compile_project_full(
    start_path: &Path,
    progress: Option<tokio::sync::mpsc::UnboundedSender<ProgressEvent>>,
) -> Result<ProjectOutput>
```

Ces deux fonctions encapsulent :
- Trouver `knot.toml` et la racine projet
- Compiler tous les fichiers `.knot` (main + includes)
- Assembler avec includes injection + codly config + marqueurs `BEGIN-FILE`/`END-FILE`

`knot-cli::build_project` est refactoré pour déléguer à `compile_project_full`.

---

## Phase 2 — État de l'overlay tinymist dans le LSP

### Principe

Pour que tinymist mette à jour le browser sans débounce FSEvents (macOS), il faut
utiliser le mode "overlay" LSP :
- `textDocument/didOpen` → tinymist entre en mode in-memory pour ce fichier
- `textDocument/didChange` → mise à jour instantanée du rendu

L'overlay a un état : il faut savoir si `didOpen` a été envoyé avant d'envoyer `didChange`.

### Enum `TinymistOverlay`

```rust
/// État de l'overlay in-memory de tinymist pour main.typ
enum TinymistOverlay {
    /// textDocument/didOpen pas encore envoyé.
    /// tinymist surveille le fichier disque (mode normal).
    Inactive,

    /// textDocument/didOpen envoyé avec version = 1.
    /// tinymist est en mode in-memory pour ce fichier.
    /// next_version : numéro à utiliser pour le prochain textDocument/didChange.
    /// Commence à 2 (1 étant réservé au didOpen).
    Active {
        main_typ_path: PathBuf,
        next_version: u64,
    },
}
```

Remplace `preview_typ_version: Arc<AtomicU64>` dans `ServerState`.
Ce type rend explicite l'invariant : on ne peut envoyer `didChange` que si l'overlay est `Active`.

### Séquence `knot/startPreview`

```
knot/startPreview
  │
  ├─ 1. compile_project_phase0(knot_path)       [spawn_blocking]
  │      → ProjectOutput { typ_content, main_typ_path, ... }
  │
  ├─ 2. write(main_typ_path, typ_content)        [disque]
  │
  ├─ 3. textDocument/didOpen(uri, version=1, text=typ_content)
  │      overlay = Active { main_typ_path, next_version: 2 }
  │
  ├─ 4. workspace/executeCommand tinymist.doStartPreview
  │      → static_server_port
  │      preview_info = Some((task_id, port))
  │
  └─ 5. textDocument/didChange(version=2, text=typ_content)
         overlay.next_version = 3
         → Phase 0 visible dès l'ouverture du browser
```

---

## Phase 3 — Pipeline de mise à jour preview dans `did_save`

### États du cycle de compilation

```rust
/// Ce qu'un cycle de compilation produit, étape par étape.
enum PreviewUpdate {
    /// Phase 0 prête : cache hits + placeholders, aucune exécution.
    /// Doit être affiché immédiatement (< 50ms après la sauvegarde).
    Phase0 {
        content: String,
        generation: u64,
    },

    /// Un chunk vient de terminer son exécution.
    /// Le contenu inclut tous les résultats reçus jusqu'ici.
    ChunkComplete {
        content: String,
        generation: u64,
    },

    /// Tous les chunks sont exécutés. Résultat définitif.
    /// Déclenche aussi la mise à jour des diagnostics.
    Final {
        content: String,
        generation: u64,
    },
}
```

### Séquence `did_save`

```
did_save(uri)
  │
  ├─ 1. forward_to_tinymist(DID_SAVE, uri)       [masque syntaxique knot-virtual://]
  │
  ├─ 2. generation = compile_generation.fetch_add(1) + 1
  │
  ├─ 3. spawn_blocking : compile_project_phase0(knot_path)
  │      → apply_update(Phase0 { content, generation })
  │
  ├─ 4. Créer canal : (progress_tx, progress_rx) = unbounded_channel()
  │
  ├─ 5. spawn_blocking : compile_project_full(knot_path, Some(progress_tx))
  │      [tourne en parallèle des étapes suivantes]
  │
  ├─ 6. Pour chaque ProgressEvent dans progress_rx :
  │      → assembler contenu partiel (résultats reçus + placeholders restants)
  │      → apply_update(ChunkComplete { content, generation })
  │
  └─ 7. Quand compile_project_full termine :
         → apply_update(Final { content, generation })
         → sync_with_cache(uri)
         → publish_combined_diagnostics(uri)
```

### Fonction `apply_update` (simple et déterministe)

```rust
async fn apply_update(&self, update: PreviewUpdate) {
    let (content, generation) = match &update {
        PreviewUpdate::Phase0 { content, generation }       => (content, *generation),
        PreviewUpdate::ChunkComplete { content, generation } => (content, *generation),
        PreviewUpdate::Final { content, generation }         => (content, *generation),
    };

    // Abandon si une sauvegarde plus récente a pris le relais
    if self.state.compile_generation.load(SeqCst) != generation {
        return;
    }

    // 1. Écrire sur disque (utile pour knot watch, typst, etc.)
    let _ = std::fs::write(&main_typ_path, content);

    // 2. Si l'overlay est actif : notifier tinymist en mémoire (instantané)
    if let TinymistOverlay::Active { next_version, .. } = &mut overlay {
        let version = *next_version;
        *next_version += 1;
        proxy.send_notification(DID_CHANGE, json!({
            "textDocument": { "uri": uri, "version": version },
            "contentChanges": [{ "text": content }]
        })).await;
    }
}
```

---

## Phase 4 — `knot watch` (CLI, non-progressif)

`knot watch` produit un PDF. Un PDF est atomique — pas de "demi-PDF".
Donc : pas de streaming, pas de tinymist, pas de browser.

```
knot watch :
  détecter changement .knot
  → compile_project_full(start_path, None)   [sans progress sender]
  → écrire .typ
  → typst compile → PDF
  → PDF viewer (Skim, etc.) auto-refresh
```

Implémentation probable : wrappé autour de `typst watch` pour la partie PDF,
avec notre compilation `.knot → .typ` en amont.

---

## Résumé des responsabilités

| Responsabilité | knot-core | knot-cli | knot-lsp |
|---|---|---|---|
| Compilation chunks (R/Python) | ✅ | ❌ | ❌ |
| Assemblage projet (.typ complet) | ✅ | délègue | délègue |
| Génération PDF | ❌ | ✅ | ❌ |
| Watch fichiers → PDF | ❌ | ✅ | ❌ |
| Overlay tinymist (didOpen/didChange) | ❌ | ❌ | ✅ |
| Preview browser (start/stream/update) | ❌ | ❌ | ✅ |
| Sync forward/backward | ✅ (sync.rs) | ❌ | délègue |
