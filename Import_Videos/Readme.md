# Import_Videos / Video_Youtube_Downloader

## FR

Telecharge des videos MP4 (flux fusionne) ou de l'audio MP3 depuis YouTube via
`yt-dlp` et `ffmpeg`. Supporte un fichier `cookies.txt` exporte depuis le navigateur pour
acceder aux contenus restreints.

> Utilise cet outil uniquement sur des contenus pour lesquels tu disposes des droits.

### Prerequis
- Python 3.9+
- Un moteur JavaScript compatible avec yt-dlp :
  - Deno 2.3+ (recommande), ou
  - Node.js 22+
- `ffmpeg` disponible dans le `PATH`
  - Windows : `winget install Gyan.FFmpeg` ou `choco install ffmpeg`
  - macOS : `brew install ffmpeg`
  - Linux : `sudo apt-get install ffmpeg`
- Dependances Python :
  ```bash
  python -m pip install -r requirements.txt
  ```

### Exporter cookies.txt (optionnel)
1. Connecte-toi a YouTube dans Firefox.
2. Installe l'extension https://addons.mozilla.org/fr/firefox/addon/cookies-txt/.
3. Exporte les cookies et copie `cookies.txt` dans `Import_Videos/`.

Si `cookies.txt` est absent ou invalide, le script continue sans cookies (utile pour les
videos publiques).

### Utilisation
```bash
# Video MP4 -> dossier Downloads par defaut
python import_yt_dlp.py "https://www.youtube.com/watch?v=<ID>"

# Audio MP3 -> dossier personnalise
python import_yt_dlp.py "https://youtu.be/<ID>" "D:/Imports" audio

# Audio MP3 -> dossier Downloads par defaut
python import_yt_dlp.py "https://youtu.be/<ID>" audio
```
Arguments :
1. URL YouTube (obligatoire)
2. Dossier de sortie ou mode `video`/`audio`/`mp3` (optionnel, defaut = `~/Downloads`)
3. Mode `video`, `audio` ou `mp3` (optionnel, defaut = `video`)

Si seule l'URL est fournie, le script demande le mode interactivement.

Profils de telechargement :
- Video : `bestvideo[ext=mp4][vcodec^=avc1]+bestaudio[ext=m4a]` puis fusion en MP4.
- Audio : piste francaise M4A saine en priorite, puis anglaise, autre piste non-ASL ou WebM en repli, convertie en MP3 haute qualite.
- `mp3` reste un alias du mode `audio`.
- Nom de fichier audio : `%(title)s [audio-mp3].mp3`.

### Generer le .exe
Depuis PowerShell :
```powershell
cd O:\TOOLS\Depot\DPER_TOOLS\Import_Videos
.\build.bat
```

Le script installe les dependances Python, installe `pyinstaller`, puis genere :
```text
O:\TOOLS\Depot\DPER_TOOLS\Import_Videos\dist\import-videos.exe
```

Commande equivalente sans `build.bat` :
```powershell
python -m pip install -r requirements.txt
python -m pip install pyinstaller
python -m PyInstaller --clean --noconfirm --onefile --console --icon "Youtube_3D.ico" import_yt_dlp.py -n import-videos
```

Apres generation, garde `cookies.txt` dans le meme dossier que `import-videos.exe`
si YouTube demande les cookies. `ffmpeg` et Deno/Node doivent rester accessibles
dans le `PATH` de Windows.

### Depannage
- **ffmpeg introuvable** : installe ffmpeg puis rouvre un terminal.
- **Cookies invalides** : re-exporte `cookies.txt` depuis le bon compte.
- **Erreurs 429/quota** : active les cookies ou patiente.

Branches recommandees : `feature/import_videos-<ticket>` depuis `dev`.

---

## EN

Download merged MP4 video or MP3 audio from YouTube using `yt-dlp` and `ffmpeg`.
Optionally rely on a browser-exported `cookies.txt` to access age/region restricted
content.

> Use this tool only for content you are authorized to download.

### Requirements
- Python 3.9+
- A JavaScript runtime supported by yt-dlp:
  - Deno 2.3+ (recommended), or
  - Node.js 22+
- `ffmpeg` in your `PATH`
  - Windows: `winget install Gyan.FFmpeg` or `choco install ffmpeg`
  - macOS: `brew install ffmpeg`
  - Linux: `sudo apt-get install ffmpeg`
- Python dependencies:
  ```bash
  python -m pip install -r requirements.txt
  ```

### Exporting cookies.txt (optional)
1. Sign in to YouTube with Firefox.
2. Install the https://addons.mozilla.org/fr/firefox/addon/cookies-txt/ add-on.
3. Export cookies and drop `cookies.txt` into `Import_Videos/`.

When `cookies.txt` is missing/invalid the script falls back to public-only downloads.

### Usage
```bash
# Default MP4 download -> Downloads folder
python import_yt_dlp.py "https://www.youtube.com/watch?v=<ID>"

# MP3 audio -> custom output folder
python import_yt_dlp.py "https://youtu.be/<ID>" "D:/Imports" audio

# MP3 audio -> default Downloads folder
python import_yt_dlp.py "https://youtu.be/<ID>" audio
```
Arguments:
1. YouTube URL (required)
2. Output directory or `video`/`audio`/`mp3` mode (optional, default `~/Downloads`)
3. `video`, `audio`, or `mp3` mode (optional, default `video`)

When only the URL is supplied, the script asks for the mode interactively.

Download profiles:
- Video: `bestvideo[ext=mp4][vcodec^=avc1]+bestaudio[ext=m4a]` merged into MP4.
- Audio: healthy French M4A track first, then English, another non-ASL track, or WebM fallback, converted to high-quality MP3.
- `mp3` remains an alias for `audio` mode.
- Audio output pattern: `%(title)s [audio-mp3].mp3`.

### Build the .exe
From PowerShell:
```powershell
cd O:\TOOLS\Depot\DPER_TOOLS\Import_Videos
.\build.bat
```

The build script installs the Python dependencies, installs `pyinstaller`, then
creates:
```text
O:\TOOLS\Depot\DPER_TOOLS\Import_Videos\dist\import-videos.exe
```

Equivalent command without `build.bat`:
```powershell
python -m pip install -r requirements.txt
python -m pip install pyinstaller
python -m PyInstaller --clean --noconfirm --onefile --console --icon "Youtube_3D.ico" import_yt_dlp.py -n import-videos
```

After building, keep `cookies.txt` next to `import-videos.exe` if YouTube needs
cookies. `ffmpeg` and Deno/Node must still be available in the Windows `PATH`.

### Troubleshooting
- **ffmpeg not found**: install it and reopen your shell so `PATH` refreshes.
- **Invalid cookies**: re-export `cookies.txt` while logged into the right account.
- **429/quota errors**: enable cookies or wait before retrying.

Suggested branches: `feature/import_videos-<ticket>` cut from `dev`.
