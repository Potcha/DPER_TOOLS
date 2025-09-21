# Guide de contribution – DPER_TOOLS

Merci de votre intérêt pour contribuer à **DPER_TOOLS** 🙌  
Ce dépôt est un **monorepo** qui regroupe plusieurs outils (ex. `Scan_Explorer`, `Import_Videos`).  
L’objectif : garder un tronc commun propre tout en laissant chaque outil évoluer à son rythme.

---

## 🌳 Stratégie de branches (par outil)

- **Tronc du monorepo** : `main`
- **Branche principale par outil** :
  - `scan-explorer-main` pour `DPER_TOOLS/Scan_Explorer/`
  - `import-videos-main` pour `DPER_TOOLS/Import_Videos/`
- **Branches de dev (feature/fix)** : créez-les **à partir de la branche principale de l’outil**, par ex. :
  - `feat/scan-explorer-<slug>`
  - `fix/import-videos-<slug>`

**Important** : une branche dédiée à un outil ne doit modifier **que** le dossier de cet outil.

---

## ✅ Règle “répertoire uniquement”

Chaque PR vers la branche principale d’un outil (**ex.** `import-videos-main`) ne doit contenir que des changements sous le **répertoire de l’outil** concerné :

- PR vers `import-videos-main` ➜ fichiers sous `DPER_TOOLS/Import_Videos/**`
- PR vers `scan-explorer-main` ➜ fichiers sous `DPER_TOOLS/Scan_Explorer/**`

Des workflows GitHub (YAML) vérifient automatiquement que les fichiers modifiés **restent dans le bon dossier**.  
Si des fichiers hors périmètre sont modifiés, la CI échoue et la PR ne peut pas être fusionnée.

---

## 🔐 Branch protection & revues

Sur les branches protégées (`main`, `scan-explorer-main`, `import-videos-main`) :

- **PR obligatoire** (pas de push direct)
- **1 review minimum** (plus si requis)
- **Statuts CI au vert** (lint/tests/“path guard”)
- (Recommandé) **CODEOWNERS** pour assigner des reviewers par répertoire

Exemple de `CODEOWNERS` (à la racine) :
- [ ] DPER_TOOLS/Scan_Explorer : Michel relecteur

- [ ] /DPER_TOOLS/Scan_Explorer/ @Potcha

- [ ] DPER_TOOLS/Import_Videos : Michel relecteur

- [ ] /DPER_TOOLS/Import_Videos/ @Potcha

---

## 🧭 Flux de travail type

1. Créez votre branche de dev :
   ```bash
   # Exemple pour Scan_Explorer
   git checkout scan-explorer-main
   git checkout -b feat/scan-explorer-<ma-feature>
Commits atomiques, messages clairs (voir plus bas).

Ouvrez une PR vers la branche main de l’outil (ex. scan-explorer-main).

La CI vérifie :

que les fichiers modifiés sont dans le bon dossier (path guard),

que le build/tests passent (si définis).

Après review et merge :

Tag de release par outil : scan-explorer-vX.Y.Z, import-videos-vX.Y.Z

(optionnel) Ouvrir une PR de synchronisation vers main du monorepo.

🧩 Conventions de commit (recommandé)
Adoptez Conventional Commits pour des changelogs propres :

feat(Scan_Explorer): …

fix(Import_Videos): …

docs(root): …

chore: …, refactor: …, test: …

Exemples :

````scss
Copier le code (scss)
feat(Import_Videos): support cookies.txt + audio-only
fix(Scan_Explorer): lien "copier le chemin" sur sous-rapport
docs: ajoute guide d’installation ffmpeg (Windows/Mac/Linux)
````
🔧 Style & qualité
Python : privilégier code clair, fonctions courtes, erreurs gérées.

(Optionnel) Lint/format :

pip install black ruff puis black . && ruff .

Tests : si vous ajoutez des comportements sensibles, joignez des tests (même simples).

🚀 Releases & CI
Tags par outil :

scan-explorer-v0.1.0

import-videos-v0.1.0

Les workflows GitHub (par outil) peuvent :

builder un .exe (PyInstaller),

attacher l’artefact à la Release (automatique sur tag).

Les workflows sont filtrés par chemins : seules les modifs du dossier d’un outil déclenchent son build.

🐞 Issues & PR
Issues : merci d’inclure OS, version Python, étapes pour reproduire.

PR : cochez la checklist :
- [ ] Ma PR cible la branche principale de l’outil (pas main du monorepo)
- [ ]  Les fichiers modifiés sont uniquement dans le dossier de l’outil
- [ ] La CI est verte
- [ ] J’ai mis à jour la doc si besoin


📜 Licence
Projet sous MIT – voir LICENSE.
Merci pour vos contributions !

---
