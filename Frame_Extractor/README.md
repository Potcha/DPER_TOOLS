# Frame_Extractor

## FR

Interface Tkinter pour transformer une video en sequence d'images exploitables
(par exemple pour un flux de photogrammetrie / scanner 3D).

### Fonctionnalites
- Extraction de toutes les frames ou 1 frame sur N (`every_n`).
- Choix du format (PNG ou JPG) et de la qualite JPG.
- Prefixe personnalisable, zero padding et index de depart.
- Apercu miniature de la video chargee.
- Extractions realisees dans un thread pour garder l'UI reactive.

### Prerequis
- Python 3.9+ (Windows teste, macOS/Linux devraient aussi fonctionner).
- Dependances Python :
  ```bash
  python -m pip install -r requirements.txt
  ```
  (contient `opencv-python` et `pillow`).

### Utilisation
```bash
python frame_extractor_gui.py
```
1. Selectionner la video source.
2. Choisir le dossier de sortie.
3. Ajuster les options (format, prefixe, `every_n`, etc.).
4. Cliquer sur **Extraire**. Les images sont enregistrees dans le dossier cible.

### Build Windows (optionnel)
PyInstaller est deja configure via `frame_extractor_gui.spec`.
```bash
pip install pyinstaller
pyinstaller frame_extractor_gui.spec
```
L'executable apparait dans `dist/`.

### Branches / releases
- Branches de travail : `feature/frame_extractor-<ticket>`.
- Tags : `frame-extractor-vX.Y.Z`.

Pour des ameliorations UI (progression, filtrage FPS, presets), creer une issue etiquetee `Frame_Extractor`.

---

## EN

Tkinter GUI that turns a video into a sequence of images (useful for photogrammetry or
3D scanning prep work).

### Features
- Extract every frame or 1 frame out of N (`every_n`).
- Choose PNG or JPG plus JPG quality settings.
- Custom prefix, zero padding, and start index.
- Thumbnail preview of the loaded video.
- Extraction runs in a background thread to keep the UI responsive.

### Requirements
- Python 3.9+ (tested on Windows; macOS/Linux should also work).
- Python dependencies:
  ```bash
  python -m pip install -r requirements.txt
  ```
  (includes `opencv-python` and `pillow`).

### Usage
```bash
python frame_extractor_gui.py
```
1. Select the video file.
2. Choose the output folder.
3. Adjust options (format, prefix, `every_n`, etc.).
4. Click **Extract** to export the frames.

### Windows build (optional)
PyInstaller config is shipped (`frame_extractor_gui.spec`).
```bash
pip install pyinstaller
pyinstaller frame_extractor_gui.spec
```
Executable appears in `dist/`.

### Branching / releases
- Working branches: `feature/frame_extractor-<ticket>`.
- Tags: `frame-extractor-vX.Y.Z`.

Open an issue labeled `Frame_Extractor` for UI ideas (progress bar, filtering, presets, ...).
