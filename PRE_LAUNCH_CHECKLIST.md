### **Pense-Bête pour le Lancement Public de Knot (Version Idéale)**

**Organisation GitHub :** `knot-literate-programming`
**Dépôt GitHub :** `knot-literate-programming/knot`

---

#### **Phase 1 : Fiabilisation & Accessibilité (Durée estimée : 1-2 semaines)**

*   **[ ] Mettre en place l'Intégration Continue (CI) sur GitHub Actions :**
    *   **Configuration :** Créer un fichier `.github/workflows/ci.yml`.
    *   **Actions :** Chaque Pull Request et chaque push sur `main` doit déclencher :
        *   `cargo fmt --check`
        *   `cargo clippy -- -D warnings` (pour détecter les warnings comme des erreurs)
        *   `cargo test` (y compris les tests Python/R si possible, marqués comme `#[ignore]` dans Rust ou via des scripts dédiés)
    *   **Pourquoi :** Garantit la qualité du code et l'absence de régression.

*   **[ ] Automatiser la publication de binaires pré-compilés :**
    *   **Option 1 (`cargo-dist` - recommandé) :** Intégrer `cargo-dist` pour générer automatiquement les exécutables pour Linux, macOS (Intel/ARM), et Windows lors des "Releases" GitHub.
    *   **Option 2 (GitHub Actions manuel) :** Configurer un workflow qui compile et attache les binaires aux tags de release.
    *   **Pourquoi :** Simplifie énormément l'installation pour les non-développeurs Rust.

*   **[ ] Améliorer les messages d'erreur :**
    *   **Focus :** Prioriser les erreurs provenant des exécutions R/Python et du parsing.
    *   **Objectif :** Rendre les messages d'erreur aussi clairs et exploitables que possible, en indiquant la cause et, si possible, la ligne/colonne du problème.
    *   **Pourquoi :** Réduit la frustration et le temps de débuggage pour l'utilisateur.

---

#### **Phase 2 : Documentation Essentielle (Durée estimée : 2-3 semaines)**

*   **[ ] Créer un `README.md` (à la racine du dépôt) accrocheur :**
    *   **Slogan :** "Knot: Blazing-fast, multi-language literate programming for Typst. Seamlessly weave reproducible R & Python code into your documents."
    *   **Description concise :** Expliciter ce que Knot fait et pourquoi c'est utile.
    *   **GIF animé ou Capture d'écran :** Une démonstration visuelle rapide de Knot en action (compilation d'un `.knot` vers un `.typ` avec résultats, ou utilisation dans VSCode). C'est un *must*.
    *   **Section "Quick Start" :** Les 3-4 étapes pour commencer (installation, premier document simple, compilation).
    *   **Badges :** "Build Status", "Latest Release", "License", etc.
    *   **Liens :** Vers la documentation complète (`mdbook`), la page de release GitHub.
    *   **Pourquoi :** La première impression est cruciale.

*   **[ ] Mettre en place la documentation complète avec `mdbook` :**
    *   **Configuration :** Créer un dossier `/docs` avec un `SUMMARY.md` et des fichiers Markdown pour les sections. Configurer `mdbook` et idéalement le déployer via GitHub Pages (`knot-literate-programming.github.io/knot`).
    *   **Contenu minimum :**
        *   **Installation :** Guide détaillé pour toutes les plateformes (avec liens vers les binaires).
        *   **Tutoriel : "Votre premier document Knot"** : Un guide pas-à-pas simple et complet.
        *   **Référence du langage Knot :** Détaillez toutes les options de chunk (`#| eval:`, `cache:`, `fig-width:`, `constant:` etc.) et des expressions inline.
        *   **Guide VSCode :** Comment installer l'extension, les fonctionnalités clés (diagnostics, snippets).
        *   **Guide de contribution (`CONTRIBUTING.md`) :** Comment installer l'environnement de dev, comment lancer les tests, comment soumettre une PR.
        *   **FAQ :** Anticiper les questions courantes.
    *   **Pourquoi :** Une bonne documentation est la clé de l'adoption par la communauté.

---

#### **Phase 3 : Migration & Nettoyage (Durée estimée : 1-2 jours)**

*   **[ ] Revue finale du code et des dépôts :**
    *   **Action :** Parcourir attentivement le code, les fichiers de configuration, et l'historique Git pour s'assurer qu'aucune information sensible (identifiants, chemins locaux spécifiques, commentaires internes privés, noms d'utilisateurs liés au CNRS, etc.) n'est présente.
    *   **Pourquoi :** Pour éviter toute fuite d'information et garantir la nature publique du projet.

*   **[ ] Push final vers GitHub :**
    *   **Action :** Une fois la CI et les docs de base prêtes, effectuez le `git push` final vers `https://github.com/knot-literate-programming/knot`.
    *   **Pourquoi :** Le projet est maintenant sur GitHub.

---

#### **Phase 4 : Annonce Publique (Dès que les phases précédentes sont prêtes)**

*   **[ ] Préparer les annonces :**
    *   **Message :** Rédiger un message clair, enthousiaste et concis pour Reddit (r/Typst) et le serveur Discord Typst.
    *   **Contenu :** Présenter Knot, ses avantages clés, inclure un lien vers le dépôt GitHub, un lien vers la documentation (`mdbook`), et idéalement le GIF/vidéo de démo. Inviter aux contributions et retours.
    *   **Pourquoi :** Pour générer de l'intérêt et des premiers utilisateurs.

---

**Rappel Important :**
*   **Versionner les dépendances externes :** Assurez-vous que votre projet est résilient aux changements de versions des dépendances externes (Rust, Typst, Python, R).
*   **Tester sur différentes plateformes :** Si possible, vérifiez le bon fonctionnement des binaires et de l'outil sur les systèmes d'exploitation les plus courants.
