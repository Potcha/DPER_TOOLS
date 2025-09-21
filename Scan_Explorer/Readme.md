# ScanExplorer (V0)

Outil générique pour **parcourir une arborescence** et générer un **rapport HTML** pliable/dépliable :
- navigation dossier ↔ sous-dossier (une **page HTML par dossier**),
- **compteurs par type de fichier** (Images, Vidéos, Audio, Projets, Autres),
- bouton qui **copie le chemin** du dossier pour l’ouvrir rapidement dans l’Explorateur  
  *(Win+E → Ctrl+L → Ctrl+V → Entrée)*.

---

## Prérequis

- **Python 3.9+** (testé Windows)  
- `tkinter` (inclus dans l’installation standard de Python sur Windows)

---

## Utilisation

```bash
python scan_explorer.py
```
Un sélecteur s’ouvre → choisissez le dossier racine à analyser.

Le navigateur s’ouvre automatiquement sur le rapport.

Sorties :

- Rapport principal : rapport_contenu.html

- Sous-rapports : dossier rapports_html/
(chaque page liste les fichiers avec taille & date + bouton Copier le chemin)

### Exclusions
Copiez le modèle puis adaptez vos motifs :

````bash
Copier le code
# depuis tools/scan-explorer
copy ignore.txt.example ignore.txt   # Windows
# ou
cp ignore.txt.example ignore.txt     # macOS/Linux
````
Les motifs utilisent les wildcards fnmatch (chemins/nom de fichiers) :

````javascript
CacheClip/
OptimizedMedia/
.gallery/
.cache/
*/Temp/
*/tmp/
*/Logs/
*.tmp
*.bak
*.log
````
### Catégorisation des fichiers
Les extensions sont regroupées pour les compteurs :

- Images : .jpg .jpeg .png .gif .bmp .tiff .heic

- Vidéos : .mp4 .mov .avi .mkv .wmv

- Audio : .mp3 .wav .flac .aac .ogg

- Projets : .drp .prproj .veg .aup .blend

- Autres : tout le reste

(Modifiez les sets dans scan_explorer.py si besoin.)

### Build .exe (optionnel)
- Générer un binaire autonome Windows avec PyInstaller :

````bash
Copier le code
pip install pyinstaller
pyinstaller --onefile --windowed --clean scan_explorer.py -n scan-explorer
````
L’exécutable est créé dans dist/scan-explorer.exe.
````bash
# exécution simple (pas de deps)
python scan_explorer.py

# outils dev (optionnel)
python -m pip install -r requirements.txt
````
## Conseils & Dépannage
### Copie du chemin :
- si votre navigateur bloque l’accès au presse-papiers, utilisez le champ texte
affiché à côté du bouton → sélectionner / copier manuellement.

### Ouverture de fichiers : 
- les liens file:/// s’ouvrent dans le navigateur. Pour lire une vidéo non
supportée par le navigateur, utilisez le bouton Copier le chemin puis ouvrez le dossier dans l’Explorateur
et lancez la lecture avec votre lecteur (ex. VLC).

### Droits d’accès : 
- si certains dossiers ne s’affichent pas (permissions), lancez l’outil depuis une session
avec les droits suffisants.
