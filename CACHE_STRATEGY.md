# Stratégie de Cache : Objets Constants Vérifiés

## 1. Problème Adressé

La stratégie de cache actuelle sauvegarde l'intégralité de l'environnement R dans un fichier snapshot (`.RData`) pour chaque chunk. Si l'environnement contient des objets volumineux (ex: de gros dataframes), ces snapshots deviennent très lourds, menant à une utilisation excessive de l'espace disque car ces objets sont dupliqués dans le cache à chaque étape.

## 2. Philosophie de la Solution : Le Contrat "Trust but Verify"

La solution proposée est un modèle de "contrat" avec l'utilisateur pour les objets volumineux qui sont censés être immuables (en lecture seule) après leur création.

1.  **Le Contrat :** L'utilisateur déclare explicitement qu'un objet est "constant" via une option de chunk. C'est sa promesse que le code ne modifiera pas cet objet.
2.  **L'Avantage :** En échange de cette promesse, l'outil optimise drastiquement le cache en ne stockant cet objet qu'une seule fois.
3.  **La Vérification :** À la fin de l'exécution, l'outil vérifie que la promesse a été tenue. Si l'objet a été modifié, la compilation échoue avec une erreur claire, garantissant l'intégrité du résultat.

Cette approche allie performance, optimisation de l'espace disque et sécurité de la reproductibilité.

## 3. Avantages

*   **Réduction massive de l'espace disque :** Les objets volumineux ne sont stockés qu'une seule fois.
*   **Performance améliorée :** La manipulation (écriture, lecture) des snapshots d'environnement, désormais beaucoup plus petits, est plus rapide.
*   **Reproductibilité garantie :** Le système attrape à coup sûr toute modification accidentelle d'un objet "constant".
*   **Contrat utilisateur clair :** La déclaration `#| constant: ...` rend l'intention explicite et améliore la lisibilité du code.

## 4. Syntaxe Utilisateur Proposée

L'utilisateur déclare un objet comme constant dans le chunk où il est créé, via une option :

```r
```{r load_data, constant="big_dataframe"}
# ou, si on préfère un style de commentaire : #| constant: big_dataframe
big_dataframe <- read.csv("tres_gros_fichier.csv")
```

## 5. Détail des Étapes d'Implémentation

### Étape 1 : Déclaration et Parsing

L'outil doit être capable de parser l'option `#| constant: <nom_objet>` depuis les métadonnées d'un chunk.

### Étape 2 : Mise en Cache Initiale de l'Objet Constant

*   Lorsqu'un chunk avec une déclaration `#| constant: obj` est exécuté pour la première fois (cache miss) :
    a. L'exécuteur R sauvegarde **uniquement l'objet `obj`** dans un fichier binaire dédié (ex: `.rds`).
    b. Le nom de ce fichier est basé sur le **hash du contenu de `obj`** (ex: `.knot_cache/objects/{hash}.rds`), rendant le stockage "content-addressable".
    c. La métadonnée `metadata.json` est mise à jour pour mapper le nom `obj` à son hash de contenu et à son fichier de stockage.

### Étape 3 : Optimisation de la Sauvegarde des Snapshots d'Environnement

*   Pour ce chunk et tous les chunks suivants, lors de la sauvegarde du snapshot `.RData` de l'environnement :
    a. L'outil exclut temporairement tous les objets déclarés "constants" de l'environnement.
    b. Il sauvegarde le snapshot, qui est désormais beaucoup plus léger.
    c. Il restaure les objets constants dans l'environnement live si nécessaire (généralement non nécessaire car l'environnement R persiste).

### Étape 4 : Restauration de l'Environnement pour l'Exécution

*   Avant d'exécuter un chunk, l'outil prépare l'environnement R :
    *   **Cas A : "Chemin Rapide" (Exécution continue après un cache miss)**
        *   Si le chunk précédent vient d'être exécuté, l'environnement R est déjà "chaud" et contient les objets constants. Aucune action de chargement depuis le disque n'est nécessaire.
    *   **Cas B : "Restauration Complète" (Premier cache miss après une série de cache hits)**
        *   L'environnement R doit être recréé à partir de l'état du dernier chunk valide.
        *   L'outil charge d'abord le **petit snapshot `.RData`** du dernier chunk mis en cache.
        *   Ensuite, il charge les **fichiers `.rds` de tous les objets constants** déclarés jusqu'à ce point et les injecte dans l'environnement.

### Étape 5 : Vérification Finale de l'Intégrité

*   Une fois l'exécution de tous les chunks terminée, et **avant** la génération du fichier de sortie (`.typ`) :
    a. L'outil demande à R de calculer le hash final de chaque objet déclaré constant.
    b. Il compare ce hash final avec le hash initial stocké dans `metadata.json`.

### Étape 6 : Gestion des Erreurs et Sortie Atomique

*   **Si un hash a changé :**
    a. La compilation est immédiatement arrêtée.
    b. **Aucun fichier de sortie n'est généré.**
    c. Une erreur explicite est affichée : `Erreur : L'objet 'obj', déclaré comme constant, a été modifié.`
*   **Si tous les hashes sont identiques :**
    a. La compilation se poursuit et le fichier de sortie est généré.

## 6. Structure des Données du Cache (Exemple)

**Répertoire `.knot_cache/` :**

```
.knot_cache/
├── metadata.json
├── chunks/
│   ├── snapshot_chunk_1.RData  # petit
│   └── snapshot_chunk_2.RData  # petit
└── objects/
    └── a1b2c3d4e5f6.rds        # gros objet constant
```

**Fichier `metadata.json` :**

```json
{
  "document_hash": "...",
  "constant_objects": {
    "big_dataframe": "a1b2c3d4e5f6"
  },
  "chunks": [ ... ],
  "inline_expressions": [ ... ]
}
```
