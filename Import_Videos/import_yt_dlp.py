import yt_dlp
import os
import sys
import shutil

def has_valid_youtube_cookies(cookies_path):
    """Vérifie que le fichier cookies.txt contient des cookies valides pour YouTube."""
    if not os.path.exists(cookies_path):
        return False
    with open(cookies_path, 'r', encoding='utf-8') as f:
        content = f.read()
        return 'youtube.com' in content or 'google.com' in content

def download_youtube_media(video_url, output_path, download_type='video'):
    """
    Télécharge une vidéo YouTube (vidéo+audio ou audio seulement).
    """
    if not os.path.exists(output_path):
        os.makedirs(output_path)

    # Vérifie si ffmpeg est accessible dans le PATH
    ffmpeg_path = shutil.which("ffmpeg")
    if ffmpeg_path is None:
        print("❌ Erreur : ffmpeg n'est pas installé ou non accessible via la variable PATH.")
        sys.exit(1)

    # Chemin vers le fichier de cookies
    cookies_path = os.path.join(os.path.dirname(__file__), 'cookies.txt')
    cookies_valid = has_valid_youtube_cookies(cookies_path)

    if not cookies_valid:
        print("⚠️ Avertissement : Le fichier 'cookies.txt' est introuvable ou invalide.")
        print("➡️ Connecte-toi à YouTube dans Firefox, puis utilise l'extension 'cookies.txt' pour exporter les cookies.")
        print("   ▶️ https://addons.mozilla.org/fr/firefox/addon/cookies-txt/")
        print("   (le fichier doit être placé dans le même dossier que ce script)")
        use_cookies = False
    else:
        use_cookies = True

    # Options communes
    common_opts = {
        'outtmpl': os.path.join(output_path, '%(title)s.%(ext)s'),
        'ffmpeg_location': ffmpeg_path,
    }
    if use_cookies:
        common_opts['cookiefile'] = cookies_path

    # Options spécifiques selon le type
    if download_type == 'video':
        ydl_opts = {
            **common_opts,
            'format': 'bestvideo[ext=mp4][vcodec^=avc1]+bestaudio[ext=m4a]/bestvideo[ext=mp4]+bestaudio',
            'merge_output_format': 'mp4',
            'postprocessor_args': {
                'merge_output_format': ['-c:a', 'aac', '-b:a', '192k'],
            },
            'keepvideo': False,
        }
    elif download_type == 'audio':
        ydl_opts = {
            **common_opts,
            'format': 'bestaudio/best',
            'postprocessors': [{
                'key': 'FFmpegExtractAudio',
                'preferredcodec': 'mp3',
                'preferredquality': '192',
            }],
        }
    else:
        print("❌ Type de téléchargement non reconnu. Utilisez 'video' ou 'audio'.")
        sys.exit(1)

    with yt_dlp.YoutubeDL(ydl_opts) as ydl:
        ydl.download([video_url])

if __name__ == "__main__":
    default_output_path = "C:/Users/miche/Downloads"

    if len(sys.argv) > 1:
        video_url = sys.argv[1]
        output_path = sys.argv[2] if len(sys.argv) > 2 else default_output_path
        download_type = sys.argv[3].lower() if len(sys.argv) > 3 else 'video'
    else:
        video_url = input("Entrez l'URL de la vidéo YouTube à télécharger : ")
        output_path = input(f"Chemin de sauvegarde (défaut: {default_output_path}) : ") or default_output_path
        download_type = input("Type de téléchargement (video/audio, défaut: video) : ").lower() or 'video'

    if download_type not in ['video', 'audio']:
        print("❌ Type de téléchargement invalide. Utilisez 'video' ou 'audio'.")
        sys.exit(1)

    download_youtube_media(video_url, output_path, download_type)
    print(f"\n✅ {'Vidéo' if download_type == 'video' else 'Audio'} téléchargé(e) avec succès !")
