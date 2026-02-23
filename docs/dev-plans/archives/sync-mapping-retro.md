# Retour d'Expérience : Synchronisation (Sync Mapping)

Ce document résume les succès et les défis rencontrés lors de l'implémentation de la synchronisation bidirectionnelle entre Knot, Typst et le PDF.

## 1. Succès : Backward Sync (PDF → .knot)
L'implémentation du mapping inverse est stable et performante.

### Architecture
- **Moteur Rust** : Le binaire `knot` contient toute la logique de mapping. Il gère l'imbrication des fichiers via une pile de blocs et utilise des marqueurs d'injection pour `main.knot`.
- **URI Handler** : L'extension VS Code expose un protocole `vscode://knot-dev.knot/jump`.
- **Furtivité** : Lorsqu'un saut est détecté vers un fichier `.typ`, l'extension intercepte l'événement, ferme l'onglet `.typ` et ouvre le `.knot` source en moins de 50ms.

### Points forts
- Précision à la ligne près, même dans l'introduction et la conclusion de `main.knot`.
- Rapidité extrême grâce à l'assemblage en mémoire et aux regex statiques en Rust.

## 2. Défis : Forward Sync (.knot → Preview)
La synchronisation automatique du défilement de la preview vers la position du curseur dans le `.knot` s'est révélée instable.

### Problèmes rencontrés
- **Focus VS Code** : Pour que Tinymist scrolle, le fichier `.typ` doit être l'éditeur actif. Cela impose une bascule d'onglets (`.knot` -> `.typ` -> `.knot`) qui provoque un scintillement (flickering) désagréable.
- **Paresse de Tinymist** : La commande `typst-preview.sync` ne réagit pas toujours aux changements de sélection faits par programme. Elle semble optimisée pour les interactions humaines (clics réels).
- **Conflits de Listeners** : L'automatisation du forward sync entre en conflit avec le listener du backward sync, créant des boucles de focus.

### Pistes pour le futur
- **Webview Propriétaire** : Afficher le PDF dans une Webview contrôlée par Knot pour piloter le scroll via JavaScript sans toucher aux onglets de l'éditeur.
- **LSP Proxy** : Intercepter les messages entre Tinymist et VS Code au niveau du protocole.

## 3. Conclusion
Le **Backward Sync** est prêt pour la production. Le **Forward Sync** est conservé comme commande manuelle (`Cmd+K, S`) mais désactivé en automatique pour préserver la stabilité de l'interface.
