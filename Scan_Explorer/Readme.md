# Scan_Explorer (v0)

## FR

Outil pour parcourir une arborescence locale et generer un rapport HTML pliable.
Pratique pour inventorier rushs audio/video, projets, assets...

### Fonctionnalites
- Navigation dossier -> sous-dossiers, une page HTML par dossier.
- Compteurs par type de fichier (images, videos, audio, projets, autres).
- Bouton pour copier le chemin du dossier dans le presse-papiers.
- Fichier `ignore.txt` optionnel pour exclure caches/temp.

### Prerequis
- Python 3.9+
- Tkinter (inclus dans l'installation standard Windows/macOS).

### Installation rapide
```bash
python -m pip install -r requirements.txt  # optionnel : outils dev
```

### Utilisation
```bash
python scan_explorer.py
```
1. Choisir le dossier racine dans la fenetre qui s'ouvre.
2. Le navigateur s'ouvre automatiquement sur `rapport_contenu.html`.
3. Les sous-rapports sont crees dans `rapports_html/` (une page par dossier).

### Exclusions
```bash
copy ignore.txt.example ignore.txt   # Windows
cp ignore.txt.example ignore.txt     # macOS/Linux
```
Motifs possibles :
```
CacheClip/
OptimizedMedia/
.gallery/
.cache/
*/Temp/
*/Logs/
*.tmp
*.bak
*.log
```

### Build .exe (optionnel)
```bash
pip install pyinstaller
pyinstaller --onefile --windowed --clean scan_explorer.py -n scan-explorer
```
Executable Windows dans `dist/scan-explorer.exe`.

### Conseils / depannage
- Si le navigateur bloque la copie presse-papiers, utiliser le champ texte a cote du bouton.
- Pour ouvrir un dossier : bouton "Copier le chemin" -> `Win+E`, `Ctrl+L`, `Ctrl+V`, `Enter`.
- Droits insuffisants ? lancer le script depuis un shell avec les autorisations adequates.

Branches recommandees : `feature/scan_explorer-<ticket>`.
Tags releases : `scan-explorer-vX.Y.Z`.

---

## EN

Utility that scans a local tree and generates a collapsible HTML report. Useful to audit
rushes, assets, or project folders.

### Features
- Folder -> subfolder navigation, one HTML page per folder.
- Counters per file type (images, videos, audio, projects, others).
- Button to copy the folder path to the clipboard.
- Optional `ignore.txt` to skip cache/temp folders.

### Requirements
- Python 3.9+
- Tkinter (bundled with standard Windows/macOS Python installs).

### Quick install
```bash
python -m pip install -r requirements.txt
```

### Usage
```bash
python scan_explorer.py
```
1. Pick the root directory in the dialog window.
2. Your browser opens `rapport_contenu.html` automatically.
3. Sub-reports are stored under `rapports_html/` (one per folder).

### Exclusions
```bash
copy ignore.txt.example ignore.txt   # Windows
cp ignore.txt.example ignore.txt     # macOS/Linux
```
Sample patterns:
```
CacheClip/
OptimizedMedia/
.gallery/
.cache/
*/Temp/
*/Logs/
*.tmp
*.bak
*.log
```

### Optional Windows build
```bash
pip install pyinstaller
pyinstaller --onefile --windowed --clean scan_explorer.py -n scan-explorer
```
Binary available in `dist/scan-explorer.exe`.

### Tips / troubleshooting
- If clipboard access is blocked, use the text field next to the button.
- To open a folder quickly: click "Copy path" -> `Win+E`, `Ctrl+L`, `Ctrl+V`, `Enter`.
- Missing folders usually mean insufficient permissions; rerun with the right privileges.

Recommended branches: `feature/scan_explorer-<ticket>`.
Release tags: `scan-explorer-vX.Y.Z`.
