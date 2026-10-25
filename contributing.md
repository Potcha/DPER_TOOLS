# Guide de contribution - DPER_TOOLS

## FR

Ce depot regroupe plusieurs outils dans un monorepo. Les branches partagees contiennent tous les outils; le travail local de chaque outil est isole dans son propre worktree.

### Branches

- `main` : version livree de l'ensemble des outils.
- `dev` : integration partagee de l'ensemble des outils. Les PR fonctionnelles y sont revues avant publication.
- `<outil>` : branche de reference par outil, nommee d'apres son dossier.
- `feature/<outil>/<slug>` : branche de travail creee depuis la branche de reference de l'outil.
- `release/<slug>` : preparation d'une livraison de `dev` vers `main`.
- `hotfix/<outil>/<slug>` : correctif urgent issu de `main`, puis integre a `dev`.

| Outil | Branche de reference | Dossier |
|---|---|---|
| DPER_MEASURE | `DPER_MEASURE` | `DPER_MEASURE/` |
| Video_Assistant | `Video_Assistant` | `Video_Assistant/` |
| Video_Youtube_Downloader | `Import_Videos` | `Import_Videos/` |
| YouTube_Banner_Helper | `YouTube_Banner_Helper` | `YouTube_Banner_Helper/` |
| Scan_Explorer | `Scan_Explorer` | `Scan_Explorer/` |
| Frame_Extractor | `Frame_Extractor` | `Frame_Extractor/` |

Les branches d'outil conservent l'historique commun du depot. Leurs changements doivent rester dans le dossier de l'outil concerne. Le worktree sparse n'affiche que ce dossier et possede son propre index Git; les fichiers staged ou non suivis d'un autre worktree ne s'y melangent pas.

### Creer un worktree isole

Depuis la racine du depot, remplacer les noms d'exemple par l'outil choisi :

```powershell
git fetch origin
git worktree add ..\worktree-DPER_MEASURE DPER_MEASURE
git -C ..\worktree-DPER_MEASURE sparse-checkout init --cone
git -C ..\worktree-DPER_MEASURE sparse-checkout set DPER_MEASURE
git -C ..\worktree-DPER_MEASURE switch -c feature/DPER_MEASURE/mon-sujet
```

Exemple pour Video_Assistant :

```powershell
git worktree add ..\worktree-Video_Assistant Video_Assistant
git -C ..\worktree-Video_Assistant sparse-checkout init --cone
git -C ..\worktree-Video_Assistant sparse-checkout set Video_Assistant
git -C ..\worktree-Video_Assistant switch -c feature/Video_Assistant/mon-sujet
```

Ouvrir le dossier du worktree correspondant dans VS Code. Ne pas developper plusieurs outils dans le meme worktree. Garder les sorties locales et donnees de mesure non suivies dans le worktree de l'outil concerne.

### Workflow de livraison

1. Mettre a jour la branche de reference de l'outil depuis `origin`.
2. Creer `feature/<outil>/<slug>` a partir de cette branche.
3. Modifier uniquement le dossier de l'outil et ses tests/documents locaux.
4. Ouvrir une PR de la branche feature vers `dev`; la PR ne doit montrer que les fichiers de l'outil.
5. Apres revue et fusion, mettre a jour la branche de reference de l'outil depuis `origin/dev` pour preparer la prochaine feature.
6. Publier l'ensemble des outils par une PR de livraison de `dev` vers `main`.

Exemple de synchronisation apres la fusion :

```powershell
git fetch origin
git -C ..\worktree-DPER_MEASURE switch DPER_MEASURE
git -C ..\worktree-DPER_MEASURE merge --ff-only origin/dev
git push origin DPER_MEASURE
```

Une PR vers `main` n'est pas utilisee pour livrer une feature isolee. `main` est actualisee depuis `dev` lors d'une livraison globale.

### Controle du perimetre et qualite

- Verifier les fichiers de la PR avant de la soumettre : ils doivent tous appartenir au dossier de l'outil.
- Les changements transverses (workflow CI, guide racine, configuration partagee) passent par une PR separee et expliquent les outils concernes.
- Ajouter ou lancer les tests pertinents pour l'outil; preciser les validations manuelles.
- Conventional Commits recommandes, par exemple `feat(DPER_MEASURE): regrouper les mesures par type`.
- Conserver les scripts et tests dans le dossier de l'outil quand cela est possible.

### Releases et support

- Tags : `<tool>-vX.Y.Z`, par exemple `dper-measure-v0.3.0`.
- Documenter les changements majeurs dans le README de l'outil et dans la livraison.
- Pour une issue, indiquer l'outil, le dossier, l'OS, les commandes, les logs et les etapes de reproduction.

---

## EN

This repository is a monorepo. Shared branches contain all tools, while local work for each tool is isolated in its own worktree.

### Branches

- `main`: released state for all tools.
- `dev`: shared integration for all tools; feature PRs are reviewed here before release.
- `<tool>`: long-lived reference branch named after its folder.
- `feature/<tool>/<slug>`: work branch created from that tool's reference branch.
- `release/<slug>`: prepare a release from `dev` to `main`.
- `hotfix/<tool>/<slug>`: urgent fix from `main), then integrate it into `dev`.

| Tool | Reference branch | Folder |
|---|---|---|
| DPER_MEASURE | `DPER_MEASURE` | `DPER_MEASURE/` |
| Video_Assistant | `Video_Assistant` | `Video_Assistant/` |
| Video_Youtube_Downloader | `Import_Videos` | `Import_Videos/` |
| YouTube_Banner_Helper | `YouTube_Banner_Helper` | `YouTube_Banner_Helper/` |
| Scan_Explorer | `Scan_Explorer` | `Scan_Explorer/` |
| Frame_Extractor | `Frame_Extractor` | `Frame_Extractor/` |

Tool branches retain the shared repository history, but their changes must stay within the relevant tool folder. A sparse worktree displays only that folder and has its own Git index, preventing staged or untracked files from another tool worktree from appearing there.

### Create an isolated worktree

From the repository root:

```powershell
git fetch origin
git worktree add ..\worktree-Frame_Extractor Frame_Extractor
git -C ..\worktree-Frame_Extractor sparse-checkout init --cone
git -C ..\worktree-Frame_Extractor sparse-checkout set Frame_Extractor
git -C ..\worktree-Frame_Extractor switch -c feature/Frame_Extractor/my-change
```

Open the matching worktree folder in VS Code. Do not develop multiple tools in the same worktree. Keep local outputs and untracked measurement data in the corresponding tool worktree.

### Delivery workflow

1. Update the tool reference branch from `origin`.
2. Create `feature/<tool>/<slug>` from that branch.
3. Change only the tool folder and its local tests/docs.
4. Open a PR from the feature branch to `dev`; its changed files must belong only to that tool.
5. After review and merge, synchronize the tool reference branch with `origin/dev`.
6. Release all tools through a release PR from `dev` to `main`.

Do not send isolated features directly to `main`. `main` is updated from `dev` for a combined release.

### Scope and quality

- Check the PR file list before submitting; all files should belong to its tool folder.
- Cross-cutting changes (CI, root guide, shared configuration) use a separate PR describing affected tools.
- Add or run relevant tests and state the manual checks performed.
- Conventional Commits are recommended, for example `feat(DPER_MEASURE): group measurements by type`.
- Keep tool scripts and tests within their folder when practical.

### Releases and support

- Tags: `<tool>-vX.Y.Z`, for example `dper-measure-v0.3.0`.
- Document major changes in the tool README and release notes.
- For issues, include the tool, folder, OS, commands, logs, and reproduction steps.
