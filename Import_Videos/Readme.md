# Import_Videos / Video_Youtube_Downloader

## FR

Telecharge des videos MP4 (flux fusionne) ou de l'audio MP3 depuis YouTube via `yt-dlp`
et `ffmpeg`. Supporte un fichier `cookies.txt` exporte depuis le navigateur pour
acceder aux contenus restreints.

> Utilise cet outil uniquement sur des contenus pour lesquels tu disposes des droits.

### Prerequis
- Python 3.9+
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
```
Arguments :
1. URL YouTube (obligatoire)
2. Dossier de sortie (optionnel, defaut = `~/Downloads`)
3. Mode `video` ou `audio` (optionnel, defaut = `video`)

Profils de telechargement :
- Video : `bestvideo[ext=mp4][vcodec^=avc1]+bestaudio[ext=m4a]` puis fusion en MP4.
- Audio : `bestaudio/best` puis conversion MP3 192 kbps.
- Nom de fichier : `%(title)s.%(ext)s` dans le dossier cible.

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
```
Arguments:
1. YouTube URL (required)
2. Output directory (optional, default `~/Downloads`)
3. Mode `video` or `audio` (optional, default `video`)

Download profiles:
- Video: `bestvideo[ext=mp4][vcodec^=avc1]+bestaudio[ext=m4a]` merged into MP4.
- Audio: `bestaudio/best` converted to MP3 192 kbps.
- Output pattern: `%(title)s.%(ext)s`.

### Troubleshooting
- **ffmpeg not found**: install it and reopen your shell so `PATH` refreshes.
- **Invalid cookies**: re-export `cookies.txt` while logged into the right account.
- **429/quota errors**: enable cookies or wait before retrying.

Suggested branches: `feature/import_videos-<ticket>` cut from `dev`.
