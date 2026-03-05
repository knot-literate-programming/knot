# Architecture Async/Parallèle — Compilation Progressive

**Status:** 🟡 En cours — Étape 0 ✅ Étape 1 ✅ Refactoring ✅
**Date de réflexion :** 2026-02-22
**Branche cible :** `feat/progressive-compilation`
**Prérequis :** Fondations posées dans `refactor/foundations` (v0.2.4)

> Ce document complète et précise [`progressive-compilation.md`](./progressive-compilation.md),
> qui décrivait la vision initiale (modèle séquentiel). La vision définitive est async et parallèle.

---

## 1. Vision

Personne n'utilise Knot aujourd'hui — on peut concevoir l'architecture cible sans contrainte de
compatibilité ascendante. L'objectif est une compilation **vraiment async et parallèle** dès la
première implémentation, pas une version synchrone qu'on retrofiterait ensuite.

---

## 2. Modèle de parallélisme

### 2.1 Contrainte fondamentale : la linéarité par langage

Les chunks d'un même langage forment une **chaîne d'état** : le chunk N s'exécute dans
l'environnement laissé par le chunk N-1 (snapshots, variables, objets constants). Cette dépendance
est irréductible — elle est inhérente au modèle d'exécution de R et Python.

```
R:      [c1] ──→ [c2] ──→ [c3]     séquentiel (état partagé R)
Python: [c1] ──→ [c2]              séquentiel (état partagé Python)
```

### 2.2 Parallélisme inter-langages

En revanche, les chaînes de différents langages sont **totalement indépendantes**. R et Python
peuvent s'exécuter simultanément sans aucune coordination.

```
plan_pass()
     ↓
group_by_language()
     ↓
std::thread::scope(
    run_language_chain("r",      r_nodes),     ← thread OS A
    run_language_chain("python", py_nodes),    ← thread OS B
)
     ↓
assemble_pass(all_results, source)             ← réassemblage en ordre document
```

> **Note d'implémentation :** `tokio::join!` n'est pas utilisé car `knot-core` est entièrement
> synchrone — les executors communiquent via des pipes POSIX bloquants (stdin/stdout).
> `std::thread::scope` offre le parallélisme inter-langages sans runtime async.

### 2.3 Parallélisme inter-documents

Les instances `Compiler` sont indépendantes — chaque `.knot` peut être compilé dans sa propre tâche
tokio, sans partage d'état.

```
doc1.knot ──→ Compiler A ──→ ExecutedNode[] ─┐
doc2.knot ──→ Compiler B ──→ ExecutedNode[] ─┼──→ tinymist
doc3.knot ──→ Compiler C ──→ ExecutedNode[] ─┘
```

L'architecture est naturellement scalable : le nombre de tâches parallèles est borné par le nombre
de langages × le nombre de documents ouverts.

### 2.4 Cascade Inert en contexte async

Dans le modèle async, la cascade d'erreurs devient **locale à chaque tâche de langage** :
- Si R-chunk2 échoue → R-chunk3, R-chunk4 deviennent Inert (dans la tâche R)
- Python n'est pas affecté (dans sa propre tâche)
- Pas besoin d'un `broken_languages` global : chaque tâche gère son propre état d'erreur

---

## 3. Streaming vers tinymist (live preview)

### Séquence complète

```
T=0ms   save .knot
T=~5ms  parse + plan_pass
             ↓
        Assembler les CacheHits + placeholders → envoyer à tinymist
        → l'utilisateur voit le document instantanément (texte + résultats cachés)
             ↓
T=Xms   Tâches tokio en fond (une par langage)
             ↓ (chaque node complété)
        Notifier : mise à jour partielle → tinymist re-render
             ↓
T=fin   Tous les nodes complétés → output final
```

### Placeholder visuel

Pendant l'exécution d'un chunk, le document affiche un bloc `is-pending: true` (à implémenter côté
Typst/knot-package). Une fois le résultat disponible, le bloc est remplacé par le vrai contenu.

---

## 4. Prérequis techniques (à implémenter avant la compilation progressive)

### 4.1 `PlannedNode` doit être owned (bloquant ⚠️)

**Situation actuelle :** `PlannedNode<'a>` emprunte le document source via un lifetime `'a`.
**Problème :** Un type avec un lifetime non-`'static` ne peut pas être envoyé dans une tâche tokio
(`Send + 'static` requis).

**Solution :** Transformer `PlannedNode<'a>` (et `ExecutableNode<'a>`) en types entièrement owned.
Cela implique de cloner les données nécessaires (code, options, positions) lors du `plan_pass`.

```rust
// Aujourd'hui
pub struct PlannedNode<'a> {
    pub node: ExecutableNode<'a>,   // référence dans le document
    ...
}

// Cible
pub struct PlannedNode {
    pub node: OwnedExecutableNode,  // données clonées, pas de lifetime
    ...
}
```

**Impact :** Modification de `pipeline.rs`, `mod.rs` (plan_pass), et de la définition de
`ExecutableNode`.

### 4.2 `ExecutorManager` doit être splittable par langage

**Situation actuelle :** `ExecutorManager` est une structure monolithique avec accès `&mut self` —
un seul accès exclusif à la fois.

**Problème :** Pour exécuter R et Python en parallèle, chaque tâche doit avoir un accès exclusif
à son propre exécuteur, sans bloquer les autres.

**Solution :** Permettre d'extraire (move) un exécuteur individuel hors de l'`ExecutorManager` :

```rust
// Vision
let r_exec   = executor_manager.take("r")?;      // move hors du manager
let py_exec  = executor_manager.take("python")?;  // move hors du manager

tokio::join!(
    run_chain(r_exec,  r_nodes,  &cache),
    run_chain(py_exec, py_nodes, &cache),
);
```

**Impact :** Restructuration d'`ExecutorManager` (actuellement `HashMap<String, Box<dyn Executor>>`).

### 4.3 `Cache` doit tolérer les accès concurrents

**Situation actuelle :** `Cache` est mutable, passé en `&mut` exclusif.
**Problème :** Les lectures (vérification de cache, chargement de snapshots) et les écritures
(sauvegarde de résultats, snapshots) peuvent être déclenchées simultanément par plusieurs tâches.

**Options :**
- `Arc<Mutex<Cache>>` — simple, légère contention
- `Arc<RwLock<Cache>>` — lectures parallèles (probablement suffisant)
- Segmentation par langage — zero contention, plus complexe

**Décision à prendre** au moment de l'implémentation selon le profil d'accès réel.

### 4.4 `SnapshotManager` → une instance par tâche de langage

**Situation actuelle :** `SnapshotManager` est déjà segmenté par langage en interne
(`HashMap<String, String>`).
**Solution :** Simplement instancier un `SnapshotManager` par tâche de langage — chaque tâche le
possède localement, zero contention.

---

## 5. Feuille de route

```
Étape 0 — Prérequis ✅ (commit 4ec9149)
  ├── PlannedNode owned (supprimer le lifetime 'a)
  ├── ExecutorManager splittable (take() par langage)
  └── Cache concurrent-safe (Arc<Mutex<Cache>>)

Étape 1 — Cœur parallèle ✅ (commit c08f2bd)
  ├── group_by_language() — partitionnement par langage
  ├── std::thread::scope par chaîne (pas tokio — knot-core est sync)
  ├── run_language_chain() — exécution + cascade Inert locale
  └── assemble_pass en ordre document (tri par index original)

Refactoring ✅ (branche refactor/async-prereqs)
  ├── Fix : execute_pass remet tous les executors même en cas d'erreur
  ├── Suppression de ChunkProcessor (code mort depuis Étape 1)
  │     ├── ChunkExecutionState déplacé dans pipeline.rs
  │     └── resolve_options / compute_hash / format_output dans mod.rs
  └── Séparation des responsabilités dans execute_for_node
        └── restore_if_needed sorti vers run_language_chain (cycle de vie vs exécution)

Étape 2 — Streaming vers tinymist
  ├── Après plan_pass : output partiel (CacheHits + placeholders)
  ├── Mise à jour incrémentale au fil des completions
  └── is-pending: true dans le Typst/knot-package

Étape 3 — Multi-documents
  └── Déjà architecturalement OK si instances Compiler indépendantes
      (vérifier gestion des ressources partagées éventuelles)
```

---

## 6. Décisions de design ouvertes

| Question | Options | Statut |
|---|---|---|
| Runtime async | tokio (déjà dans knot-lsp) | ✅ Choix naturel |
| Parallélisme des tâches | `std::thread::scope` (knot-core sync, I/O bloquant) | ✅ Décidé |
| Synchronisation du cache | `Arc<Mutex<Cache>>` (contention faible en pratique) | ✅ Décidé |
| Cancellation | Si nouvelle save arrive pendant exécution en cours | 🔵 À concevoir |
| Backpressure | Si R tourne 60s, que faire des nouvelles saves ? | 🔵 À concevoir |

---

## 7. Ce qui ne change pas

- `plan_pass` reste synchrone et rapide (lecture cache, calcul hashes)
- `assemble_pass` reste une fonction pure sans état
- La sémantique Inert/Skip/CacheHit/MustExecute est inchangée
- La structure en trois passes est conservée — seul `execute_pass` devient async/parallel
