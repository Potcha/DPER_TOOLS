
# DPER_TOOLS

**DPER_TOOLS** est un **monorepo** d’outils utilisés pour le projet Deeper (et autres).  
Chaque outil vit dans son propre dossier sous `DPER_TOOLS/` et peut évoluer indépendamment, avec sa branche principale dédiée.

---

## 🧱 Structure

```
DPER_TOOLS/
│   README.md
│   workflows/ # CI (build, path-guards, releases par outil) 
│   LICENSE
|   CONTRIBUTING.md
└───TOOLS1
|   │   python_file
|   │   requirements
|   │   run
|   │   build
|   │   .gitignore
|   |   README.md
└───TOOLS2
    │   python_file
    │   requirements
    │   run
    │   build
    │   .gitignore
    |   README.md
...
```



- Voir les `README.md` **dans chaque dossier d’outil** pour l’installation et l’usage.
- Les releases binaires (si activées) sont attachées aux **tags par outil** (`scan-explorer-v*`, `import-videos-v*`).

---

## 🤝 Travailler en équipe / contributions

Nous acceptons issues et pull requests.  
Merci de lire d’abord **[CONTRIBUTING.md](./CONTRIBUTING.md)** qui décrit :

- la **stratégie de branches** (une branche principale par outil),
- les **garde-fous** (CI “path guard”, revues, protections),
- les **conventions de commit**,
- le **flux de release** par outil (tags + artefacts).

### Branches principales par outil

- `scan-explorer-main` → dossier `DPER_TOOLS/Scan_Explorer/`
- `import-videos-main` → dossier `DPER_TOOLS/Import_Videos/`

> Ouvrez vos PR **vers la branche principale de l’outil**, pas vers `main` (sauf PR de synchronisation).

---

## 🔍 Garde-fous CI (path guards)

Chaque outil a un workflow GitHub qui **échoue** si des fichiers hors de son dossier sont modifiés dans une PR.  
Exemple (principe) :

```yaml
on:
  pull_request:
    branches: [ import-videos-main ]
jobs:
  check-paths:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Fail if files outside DPER_TOOLS/Import_Videos changed
        run: |
          CHANGED=$(git diff --name-only ${{ github.event.pull_request.base.sha }} ${{ github.event.pull_request.head.sha }})
          for F in $CHANGED; do
            if [[ "$F" != DPER_TOOLS/Import_Videos/* ]]; then
              echo "Forbidden change outside Import_Videos: $F"
              exit 1
            fi
          done
````
Les workflows réels vivent dans .github/workflows/.

## 🛠️ Outils inclus (aperçu)
### Scan_Explorer

- But : parcourir une arborescence et générer un rapport HTML pliable/dépliable.

- Fonctions : compteurs (images/vidéos/audio/projets/autres), sous-rapports, bouton copier le chemin.

- Usage : voir DPER_TOOLS/Scan_Explorer/README.md

### Import_Videos
- But : télécharger vidéo (MP4 fusionnée) ou audio (MP3) via yt-dlp.

- Fonctions : support cookies.txt, ffmpeg, options simples en CLI.

- Usage : voir DPER_TOOLS/Import_Videos/README.md

### 📦 Releases
- Tags par outil : scan-explorer-vX.Y.Z, import-videos-vX.Y.Z

- Les workflows peuvent builder et publier les binaires associés (Windows .exe via PyInstaller).

📄 Licence 
- MIT – voir LICENSE.

🙋‍♀️ Besoin d’aide ?
- Ouvrez une Issue avec un maximum de détails (OS, Python, commandes, logs).

- Pour contribuer, lisez CONTRIBUTING.md puis créez votre branche sur la branche principale de l’outil concerné.

