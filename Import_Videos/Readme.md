# Import_Videos (V0)

Petit utilitaire basé sur **yt-dlp** pour télécharger **vidéo** ou **audio** depuis YouTube, avec :
- **pré-check** (sans download) pour diagnostiquer les erreurs d’accès,
- sélection du **pays** (contournement géo),
- utilisation des **cookies du navigateur** (ou d’un `cookies.txt`),
- fusion vidéo+audio en **MP4** (via ffmpeg) ou extraction **MP3**.

> ⚠️ Utilisez cet outil uniquement pour du contenu autorisé. Respectez les CGU et la loi.

---

## Prérequis

- **Python 3.9+**
- **ffmpeg** dans le `PATH` (fusion vidéo/audio)
  - Windows : `winget install Gyan.FFmpeg` (ou `choco install ffmpeg`)
  - macOS : `brew install ffmpeg`
  - Linux : `sudo apt-get install ffmpeg`

Installer la dépendance Python :
```bash
python -m pip install -r requirements.txt
````
Utilisation (exemples)
Vidéo MP4 (par défaut) vers le dossier Downloads :

````bash
Copier le code
python import_yt_dlp.py "https://www.youtube.com/watch?v=BaW_jenozKc"
````
Audio (MP3 192kbps) :

````bash
Copier le code
python import_yt_dlp.py "https://youtu.be/xxxx" --type audio
````
Spécifier un pays (contournement géo) :

````bash
Copier le code
python import_yt_dlp.py "https://youtu.be/xxxx" --geo US
````
Utiliser les cookies navigateur (profil par défaut) :

````bash
Copier le code
python import_yt_dlp.py "https://youtu.be/xxxx" --browser firefox
````
# (chrome|edge possibles ; désactivez avec --no-browser-cookies)
Utiliser un cookies.txt :

````bash
Copier le code
python import_yt_dlp.py "https://youtu.be/xxxx" --no-browser-cookies --cookies C:\path\cookies.txt
````
Changer le dossier de sortie :

```bash
Copier le code
python import_yt_dlp.py "https://youtu.be/xxxx" -o "Q:\MEDIA\XXXX"
````
Options
less
Copier le code
url                         URL de la vidéo YouTube
-o, --output PATH           Dossier de sortie (défaut: C:/Users/<user>/Downloads)
--type video|audio          Type de téléchargement (défaut: video)
--geo CC                    Pays (FR, US, DE…) pour contournement géo (défaut: FR)
--browser B                 Cookies depuis navigateur (firefox|chrome|edge; défaut: firefox)
--no-browser-cookies        Ne pas utiliser les cookies du navigateur
--cookies PATH              Chemin vers un cookies.txt (fallback)
Détails techniques
Pré-check : extraction d’infos sans téléchargement pour détecter :

vidéo privée / membres / restriction d’âge / région / indisponible

Vidéo : format préféré

bestvideo[ext=mp4][vcodec^=avc1]+bestaudio[ext=m4a]/bestvideo[ext=mp4]+bestaudio

sortie .mp4 via merge_output_format: mp4

Audio : bestaudio/best + FFmpegExtractAudio (MP3 192k)

Nom de fichier : %(title)s.%(ext)s dans le dossier choisi

Build .exe (optionnel, local)
bash
Copier le code
python -m pip install -r requirements.txt
python -m pip install pyinstaller
pyinstaller --onefile --console import_yt_dlp.py -n import-videos
Le binaire sera dans dist/import-videos.exe.
⚠️ La machine cible doit avoir ffmpeg dans le PATH.

Dépannage
Private/Members/Âge/Région : essayez --browser firefox (connecté au bon compte) ou --cookies <fichier>, et/ou --geo US.

ffmpeg introuvable : installez-le puis relancez dans un nouveau terminal (PATH mis à jour).
