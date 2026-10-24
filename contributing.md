# Guide de contribution - DPER_TOOLS

## FR

Merci de contribuer a DPER_TOOLS ! Ce depot regroupe plusieurs outils (Import_Videos,
Scan_Explorer, YouTube_Banner_Helper, Frame_Extractor, etc.) qui partagent les memes
pratiques de branche, de revue et de CI.

### Branches et Gitflow

- `main` : derniere version livree (tags, releases publiques).
- `dev` : integration continue de tous les outils. Toutes les PR fusionnent ici apres revue.
- `feature/<outil>-<ticket>` : branches de travail (ex: `feature/import_videos-123-audio`).
- `release/<outil>-vX.Y` : stabilisation avant publication.
- `hotfix/<outil>-...` : correctifs urgents crees depuis `main`, puis retroportes vers `dev`.

#### Repertoires et outils

| Outil                    | Dossier                  |
|--------------------------|--------------------------|
| Video_Youtube_Downloader | `Import_Videos/`         |
| YouTube_Banner_Helper    | `YouTube_Banner_Helper/` |
| Scan_Explorer            | `Scan_Explorer/`         |
| Frame_Extractor          | `Frame_Extractor/`       |

> Par principe, une branche qui traite un outil ne modifie que son dossier (plus les fichiers
> transverses explicitement necessaires).

#### Nommage recommande

- `feature/<tool>-<slug>` pour les evolutions.
- `fix/<tool>-<slug>` pour les correctifs cibles.
- `docs/<tool>-...` pour la documentation.
- Ajouter un identifiant ticket si possible (`feature/scan_explorer-456-path`).

### Workflow type

1. Mettre a jour `dev` : `git checkout dev && git pull`.
2. Creer la branche : `git checkout -b feature/<tool>-<slug>`.
3. Commits atomiques et messages clairs (Conventional Commits recommande).
4. Ouvrir une PR **vers `dev`** en remplissant le template (objectif, ticket, outil impacte, tests).
5. Laisser la CI tourner (lint/tests + path guards).
6. Obtenir au moins une review (CODEOWNERS par outil si disponible).
7. Merger une fois vert et supprimer la branche si besoin.
8. Pour publier : ouvrir `release/<tool>-vX.Y`, stabiliser, puis taguer `<tool>-vX.Y.Z`.

### Path guards & CI

- Les workflows GitHub utilisent `paths:` et echouent si des fichiers hors du dossier cible sont touches.
- Placez scripts/tests dans le dossier de l'outil : seul l'outil modifie declenche son build.
- Pour les changements transverses, coordonnez-vous et expliquez clairement votre PR.

### Commits, qualite, tests

- Conventional Commits :
  ```
  feat(Import_Videos): support cookies.txt
  fix(Frame_Extractor): handle missing pillow
  docs(root): ajoute guide gitflow
  ```
- Ajoutez des tests unitaires ou manuels documentes.
- Pensez a `black`, `ruff`, `pytest` ou autres outils locaux par dossier.

### Releases

- Tags : `<tool>-vX.Y.Z` (ex: `scan-explorer-v1.0.0`).
- Les workflows peuvent builder (PyInstaller, etc.) et attacher les artefacts aux releases GitHub.
- Documenter les changements majeurs dans le README de l'outil et/ou dans la Release.

### Issues & support

Lors de l'ouverture d'une issue, indiquez :
- Outil + dossier (`Scan_Explorer/`, `Frame_Extractor/`, ...).
- OS, version Python, commande executee, logs/stacktrace.
- Etapes de reproduction.

Merci pour votre aide afin de garder ce monorepo coherent et scalable.

---

## EN

Thank you for contributing to DPER_TOOLS! This mono-repo groups several utilities
(Import_Videos, Scan_Explorer, YouTube_Banner_Helper, Frame_Extractor, etc.) that follow the
same branching, review, and CI practices.

### Branches and Gitflow

- `main`: production-ready state (tags, published releases).
- `dev`: shared integration branch; every PR merges here after review.
- `feature/<tool>-<ticket>`: day-to-day work branches (e.g. `feature/frame_extractor-123-ui`).
- `release/<tool>-vX.Y`: short-lived stabilization branches.
- `hotfix/<tool>-...`: urgent fixes branched from `main` and backported to `dev`.

#### Folder-to-tool mapping

| Tool                    | Folder                   |
|-------------------------|--------------------------|
| Video_Youtube_Downloader| `Import_Videos/`         |
| YouTube_Banner_Helper   | `YouTube_Banner_Helper/` |
| Scan_Explorer           | `Scan_Explorer/`         |
| Frame_Extractor         | `Frame_Extractor/`       |

> Keep tool branches focused on their folder (plus explicitly needed shared files).

#### Naming guidelines

- `feature/<tool>-<slug>` for enhancements.
- `fix/<tool>-<slug>` for targeted fixes.
- `docs/<tool>-...` for documentation updates.
- Include ticket IDs when available.

### Typical workflow

1. Sync `dev`: `git checkout dev && git pull`.
2. Create your branch: `git checkout -b feature/<tool>-<slug>`.
3. Commit in small, meaningful chunks (Conventional Commits encouraged).
4. Open a PR **against `dev`** and fill the template (why, ticket, impacted tool, tests).
5. Let CI run (lint/tests + path guards).
6. Request at least one review (CODEOWNERS per folder if defined).
7. Merge when everything is green; delete the branch if desired.
8. To release, start `release/<tool>-vX.Y`, stabilize, then tag `<tool>-vX.Y.Z`.

### Path guards & CI

- Workflows leverage `paths:` and fail if changes leak outside the targeted folder.
- Keep lint/tests inside each tool directory so only relevant workflows trigger.
- For cross-tool changes, coordinate with the team and explain clearly in the PR.

### Commits, quality, tests

- Conventional Commit examples:
  ```
  feat(Import_Videos): add rate limit handling
  fix(Scan_Explorer): escape HTML in report
  docs(YouTube_Banner_Helper): clarify export flow
  ```
- Provide unit tests or describe manual test steps.
- Recommended local tooling: `black`, `ruff`, `pytest`, etc.

### Releases

- Tag format: `<tool>-vX.Y.Z` (e.g. `frame-extractor-v0.4.1`).
- Workflows can package binaries (PyInstaller, etc.) and attach them to GitHub releases.
- Document major changes in both the tool README and the GitHub Release notes.

### Issues & support

When filing an issue, please include:
- Tool/folder (`Import_Videos/`, `Frame_Extractor/`, ...).
- OS, Python version, command executed, logs/stacktrace.
- Clear reproduction steps.

Thanks for helping keep this mono-repo healthy and scalable!
