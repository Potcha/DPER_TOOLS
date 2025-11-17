# DPER_TOOLS

## FR - Apercu

DPER_TOOLS est un monorepo d'outils utilises autour du projet Deeper. Chaque outil vit dans
son propre dossier et evolue de facon independante tout en profitant de la meme infrastructure
CI/CD et des memes conventions.

### Structure rapide

```
DPER_TOOLS/
├ README.md
├ LICENSE
├ CONTRIBUTING.md
├ .github/workflows/
├ Import_Videos/ (Video_Youtube_Downloader)
├ YouTube_Banner_Helper/
├ Scan_Explorer/
└ Frame_Extractor/
```

- Voir le README.md dans chaque dossier pour l'installation detaillee.
- Les workflows GitHub vivent dans `.github/workflows/` et declenchent les tests cibles via `paths:`.

### Gitflow multi outils

- `main` : etat livre (tags, releases publiques).
- `dev` : integration continue de l'ensemble des outils.
- `feature/<outil>-<ticket>` : branches de travail (ex: `feature/import_videos-123-ui`).
- `release/<outil>-vX.Y` : stabilisation avant publication.
- `hotfix/<outil>-...` : correctifs urgents crees depuis `main`, puis retroportes vers `dev`.

Bonnes pratiques :
1. Renseigner l'outil impacte dans le template de PR et lier le ticket.
2. Limiter les changements au dossier concerne (CI path guards).
3. Taguer les releases sous la forme `<outil>-vX.Y.Z`.

### Contribution et support

- Lire [CONTRIBUTING.md](./CONTRIBUTING.md) avant toute PR.
- Utiliser le template `.github/pull_request_template.md`.
- Publier les issues en indiquant OS, outil, commandes et logs/captures.

### Outils inclus

- **Video_Youtube_Downloader (`Import_Videos/`)** : telecharge MP4/MP3 via `yt-dlp` + `ffmpeg`. Voir `Import_Videos/Readme.md`.
- **YouTube_Banner_Helper (`YouTube_Banner_Helper/`)** : previsualise les gabarits bannieres/avatars. Voir `YouTube_Banner_Helper/README.md`.
- **Scan_Explorer (`Scan_Explorer/`)** : genere un rapport HTML d'arborescence + compteurs. Voir `Scan_Explorer/Readme.md`.
- **Frame_Extractor (`Frame_Extractor/`)** : extrait les frames d'une video via une UI Tkinter. Voir `Frame_Extractor/README.md`.

### Releases et licence

- Tags par outil : `<outil>-vX.Y.Z` (ex: `scan-explorer-v1.2.0`).
- Les workflows CI peuvent packager les binaires et les attacher aux releases GitHub.
- Licence : MIT – voir [LICENSE](./LICENSE).

Pour toute question, ouvrez une issue en precisant l'outil et votre environnement.

---

## EN - Overview

DPER_TOOLS is a mono-repo that hosts multiple Deeper utilities. Each tool lives in its own folder,
evolves independently, and leverages the shared CI/CD stack and conventions.

### Repository layout

```
DPER_TOOLS/
├ README.md
├ LICENSE
├ CONTRIBUTING.md
├ .github/workflows/
├ Import_Videos/ (Video_Youtube_Downloader)
├ YouTube_Banner_Helper/
├ Scan_Explorer/
└ Frame_Extractor/
```

- Check the README inside each folder for setup details.
- GitHub workflows are stored under `.github/workflows/` and rely on `paths:` filters.

### Multi-tool Gitflow

- `main`: production-ready state (tags, public releases).
- `dev`: shared integration branch where every PR merges once approved.
- `feature/<tool>-<ticket>`: day-to-day work branches (e.g. `feature/frame_extractor-123-ui`).
- `release/<tool>-vX.Y`: short-lived stabilization branches.
- `hotfix/<tool>-...`: emergency fixes branched from `main` and backported to `dev`.

Keep in mind:
1. Fill the PR template (objective, ticket, impacted tool, tests).
2. Keep changes scoped to the tool directory (path guards will enforce it).
3. Tag releases using `<tool>-vX.Y.Z`.

### Included tools

- **Video_Youtube_Downloader (`Import_Videos/`)** – download MP4 or MP3 with `yt-dlp` + `ffmpeg`. See `Import_Videos/Readme.md`.
- **YouTube_Banner_Helper (`YouTube_Banner_Helper/`)** – preview banner/profile overlays in a browser. See `YouTube_Banner_Helper/README.md`.
- **Scan_Explorer (`Scan_Explorer/`)** – crawl a folder tree and export a collapsible HTML report. See `Scan_Explorer/Readme.md`.
- **Frame_Extractor (`Frame_Extractor/`)** – Tkinter GUI to export video frames (PNG/JPG). See `Frame_Extractor/README.md`.

### Releases & licence

- Tool-specific tags: `<tool>-vX.Y.Z` (example: `frame-extractor-v0.4.1`).
- Workflows can build binaries (PyInstaller, etc.) and attach them to GitHub releases.
- License: MIT – see [LICENSE](./LICENSE).

If you hit issues, open a GitHub Issue with details about your OS, Python version, tool, and steps performed.
