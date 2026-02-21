# Stratégie de Compilation Progressive (Knot Live Pipeline)

Ce document détaille l'architecture permettant une prévisualisation "Live" des documents Knot, en minimisant l'impact du temps d'exécution des chunks R/Python.

## 1. Le Problème
La compilation Knot est actuellement séquentielle et bloquante :
1. Knot attend que le chunk N s'exécute pour passer au chunk N+1.
2. Le fichier `main.typ` n'est généré qu'à la toute fin.
3. L'utilisateur attend plusieurs secondes pour voir une simple modification de texte.

## 2. La Solution : Pipeline à trois états
Nous introduisons une exécution non-bloquante basée sur trois états pour chaque chunk :

- **READY** : Le résultat est en cache ou vient d'être calculé. Le rendu est complet.
- **PENDING** : Le cache est vide ou invalidé. Le moteur d'exécution travaille sur ce chunk.
- **INERT** : Chunks suivant un état PENDING ou une erreur. Ils sont rendus comme du code brut (grisé) sans exécution.

## 3. Flux de travail (Workflow)

### A. Déclenchement (T=0ms)
Dès qu'une modification est détectée :
1. Knot identifie le premier chunk à ré-exécuter (Chunk K).
2. Knot génère **immédiatement** un `main.typ` "prévisionnel" où :
   - Chunks 1 à K-1 sont lus depuis le cache.
   - Chunk K est marqué `is_pending: true`.
   - Chunks K+1 à fin sont marqués `is_inert: true`.
3. Typst compile ce `main.typ` (0.3s). L'utilisateur voit ses changements de texte instantanément.

### B. Progression (T=Execution)
1. Le moteur finit l'exécution du Chunk K.
2. Knot met à jour le cache et génère un **nouveau** `main.typ` :
   - Chunk K est maintenant READY.
   - Chunk K+1 passe en `is_pending: true`.
3. Le PDF se met à jour visuellement, "découvrant" les résultats au fur et à mesure.

## 4. Intégration VS Code (Forward Sync)
Pour que la preview Tinymist suive cette progression sans perdre le focus :
- `main.typ` doit rester ouvert dans un groupe d'éditeurs en arrière-plan (éventuellement masqué).
- L'extension Knot synchronise la position du curseur du `.knot` vers le document virtuel `main.typ` pour piloter le défilement de la preview.

## 5. Rendu Visuel (Typst)
- Un bloc `pending` affichera un indicateur visuel (ex: "Exécution en cours...") avec le code source.
- Un bloc `inert` affichera le code source grisé pour indiquer qu'il attend son tour ou qu'une erreur a stoppé la chaîne.
