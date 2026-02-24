# Plan — Refactoring ExecutionOutput (session suivante)

> Fichier temporaire, ne pas merger dans master.

## Contexte

`KnotExecutor::execute()` retourne `Result<ExecutionOutput>` où `ExecutionOutput`
contient `Option<RuntimeError>`. C'est un anti-pattern *error in success* : le
compilateur ne force pas les appelants à vérifier l'erreur runtime.

Deux sortes d'erreurs existent, avec des comportements distincts :
- **Infrastructure** (process crashé, timeout) → cascade inert, **pas de cache**
- **Runtime** (exception R/Python) → cascade inert, **mise en cache**

---

## Étape A — Branche `refactor/execution-attempt`

### Objectif
Exprimer la distinction infrastructure/runtime au niveau des types.

### Nouveau design

```rust
// Dans crate::executors

pub enum ExecutionAttempt {
    Success(ExecutionOutput),     // code exécuté sans erreur
    RuntimeError(RuntimeError),   // code exécuté, erreur déterministe (cacheable)
}

// execute() retourne :
// Err(e)              = infrastructure failure (non cacheable)
// Ok(ExecutionAttempt) = exécution tentée (succès ou erreur runtime)
```

`ExecutionOutput` perd son champ `error: Option<RuntimeError>`.

### Fichiers touchés
- `crates/knot-core/src/executors/mod.rs` — trait `KnotExecutor`, `ExecutionOutput`
- `crates/knot-core/src/executors/r/execution.rs` — impl R
- `crates/knot-core/src/executors/python/execution.rs` — impl Python
- `crates/knot-core/src/compiler/execution.rs` — `execute_for_node` (call site)

### Vérifier
- Les warnings (`Vec<String>`) en cas d'erreur runtime : les garder dans
  `RuntimeError` ou créer `ExecutionAttempt::RuntimeError { error, warnings }` ?
- Les tests d'exécuteurs qui construisent `ExecutionOutput` directement.

---

## Étape B — Retour sur `refactor/async-prereqs` (après merge de A)

Avec le nouveau type, `handle_must_execute` dans `execution.rs` se simplifie
naturellement. Appliquer alors :

### Idée 1 — Early returns + style pipeline
```rust
// Avant (double-match imbriqué) :
match execute_for_node(...) {
    Ok(output) => { if let Some(error) = output.error { ... } ... }
}

// Après :
let attempt = execute_for_node(...)?; // Err infra propagé directement
match attempt {
    ExecutionAttempt::RuntimeError(e) => {
        cache_chunk_error(pn, &e, ctx.cache)?;
        return Ok(cascade(&e));
    }
    ExecutionAttempt::Success(output) => { /* chemin heureux à plat */ }
}
```

### Idée 2 — Helpers `cache_chunk_error` / `cache_chunk_result`
Pattern répété 3× → deux fonctions privées dans `execution.rs` :
```rust
fn cache_chunk_error(pn: &PlannedNode, error: &RuntimeError, cache: &Arc<Mutex<Cache>>) -> Result<()>
fn cache_chunk_result(pn: &PlannedNode, output: &ExecutionOutput, cache: &Arc<Mutex<Cache>>) -> Result<()>
```

---

## Ordre des commits

1. `refactor/execution-attempt` : nouveau type `ExecutionAttempt`, adapte R + Python + call site
2. CI complet sur cette branche, puis merge dans `refactor/async-prereqs`
3. Sur `refactor/async-prereqs` : Idées 1 + 2 (early returns + helpers)
4. CI complet, commit final
