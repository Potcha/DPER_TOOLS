# Import_Videos (V0)

Télécharge **vidéo** (MP4 fusionnée) ou **audio** (MP3) depuis YouTube avec **yt-dlp**, en utilisant
un **cookies.txt** exporté depuis ton navigateur (pour contenus restreints/âge/région).

> ⚠️ Utilise cet outil uniquement pour du contenu autorisé. Respecte les CGU et la loi.

## Prérequis
- **Python 3.9+**
- **ffmpeg** dans le `PATH`  
  - Windows : `winget install Gyan.FFmpeg` (ou `choco install ffmpeg`)  
  - macOS : `brew install ffmpeg` • Linux : `sudo apt-get install ffmpeg`
- Dépendance Python :
```bash
python -m pip install -r requirements.txt
````
cookies.txt (si nécessaire)
Connecte-toi à YouTube dans Firefox (profil utilisé).

Installe l’extension cookies.txt : https://addons.mozilla.org/fr/firefox/addon/cookies-txt/

Exporte les cookies et enregistre le fichier cookies.txt dans le dossier Import_Videos/.

Si cookies.txt est absent/invalide, le script continue sans cookies (utile pour les vidéos publiques).

Utilisation
````bash
Copier le code
# Vidéo MP4 (par défaut) → Dossier Downloads
python import_yt_dlp.py "https://www.youtube.com/watch?v=XXXXX"

# Audio MP3
python import_yt_dlp.py "https://youtu.be/XXXXX" "D:/Imports" audio

# Spécifier le dossier de sortie en 2e argument (optionnel)
python import_yt_dlp.py "<URL>" "Q:/MEDIA/Imports"
Arg 1 : URL YouTube

Arg 2 (optionnel) : dossier de sortie (défaut: C:/Users/miche/Downloads)

Arg 3 (optionnel) : video | audio (défaut: video)

Détails techniques
Vidéo : bestvideo[ext=mp4][vcodec^=avc1]+bestaudio[ext=m4a] → merge en .mp4

Audio : bestaudio/best → extraction MP3 192 kbps

Sortie : %(title)s.%(ext)s dans le dossier indiqué
````
cookies.txt détecté automatiquement s’il est présent dans le dossier du script

Dépannage
ffmpeg introuvable → installe-le puis relance une nouvelle console (PATH mis à jour).

Cookies invalides → ré-exporte cookies.txt depuis le navigateur connecté au bon compte.
