import os
import sys
import shutil
from urllib.parse import urlparse

import yt_dlp
from yt_dlp.utils import DownloadError


def get_runtime_dir():
    """Return the folder where runtime assets should be loaded from."""
    if getattr(sys, "frozen", False):
        return os.path.dirname(sys.executable)
    return os.path.dirname(os.path.abspath(__file__))


def has_valid_youtube_cookies(cookies_path):
    """Return True if cookies.txt exists and contains youtube/google cookies."""
    if not os.path.exists(cookies_path):
        return False
    with open(cookies_path, "r", encoding="utf-8") as f:
        content = f.read()
    return "youtube.com" in content or "google.com" in content


def is_youtube_url(url):
    parsed = urlparse(url)
    host = parsed.netloc.lower()
    return "youtube.com" in host or "youtu.be" in host


def download_youtube_media(video_url, output_path, download_type="video"):
    """Download a YouTube video (video+audio) or audio only."""
    if not os.path.exists(output_path):
        os.makedirs(output_path)

    ffmpeg_path = shutil.which("ffmpeg")
    if ffmpeg_path is None:
        print("Erreur : ffmpeg n'est pas installe ou non accessible via la variable PATH.")
        sys.exit(1)

    cookies_path = os.environ.get("COOKIE_FILE", os.path.join(get_runtime_dir(), "cookies.txt"))
    cookies_valid = has_valid_youtube_cookies(cookies_path)

    if not cookies_valid:
        print(f"Avertissement : Le fichier de cookies est introuvable ou invalide : {cookies_path}")
        print("Connecte-toi a YouTube dans Firefox, puis utilise l'extension 'cookies.txt' pour exporter les cookies.")
        print("    https://addons.mozilla.org/fr/firefox/addon/cookies-txt/")
        print("    (le fichier doit etre place dans le meme dossier que l'executable)")
        if is_youtube_url(video_url):
            print("\nCette video YouTube requiert generalement les cookies pour eviter les erreurs 403.")
            sys.exit(1)
        use_cookies = False
    else:
        use_cookies = True

    common_opts = {
        "outtmpl": os.path.join(output_path, "%(title)s.%(ext)s"),
        "ffmpeg_location": ffmpeg_path,
        "http_headers": {
            # User-Agent et Accept-Language aident a limiter certains 403.
            "User-Agent": (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                "AppleWebKit/537.36 (KHTML, like Gecko) "
                "Chrome/120.0.0.0 Safari/537.36"
            ),
            "Accept-Language": "fr-FR,fr;q=0.9,en-US;q=0.8,en;q=0.7",
            "Referer": video_url,
        },
        "retries": 5,
        "fragment_retries": 5,
        "concurrent_fragments": 3,
        "noplaylist": True,
        # Utilise des clients alternatifs seulement si on n'a pas de cookies.
        "extractor_args": {"youtube": {"player_client": ["android", "web"]}} if not use_cookies else {},
        # Force le geo-bypass sur la France pour les videos restreintes geo.
        "geo_bypass_country": "FR",
    }
    if use_cookies:
        common_opts["cookiefile"] = cookies_path

    if download_type == "video":
        primary_format = "bv*[ext=mp4][height<=1080][vcodec^=avc1]+ba[ext=m4a]/22"
        fallback_format = "18/best"
        post_opts = {
            "merge_output_format": "mp4",
            "keepvideo": False,
        }
    elif download_type == "audio":
        primary_format = "bestaudio/best"
        fallback_format = None
        post_opts = {
            "postprocessors": [
                {
                    "key": "FFmpegExtractAudio",
                    "preferredcodec": "mp3",
                    "preferredquality": "192",
                }
            ],
        }
    else:
        print("Type de telechargement non reconnu. Utilisez 'video' ou 'audio'.")
        sys.exit(1)

    def run_download(fmt, use_browser_cookies=False):
        opts = {**common_opts, **post_opts, "format": fmt}
        if use_browser_cookies:
            # Utilise directement les cookies du profil Firefox local si possible.
            opts.pop("cookiefile", None)
            browser = os.environ.get("BROWSER", "firefox")
            profile = os.environ.get("BROWSER_PROFILE") or None
            # yt-dlp attend un tuple (browser, profile, keyring)
            opts["cookiesfrombrowser"] = (browser, profile, None)
        with yt_dlp.YoutubeDL(opts) as ydl:
            ydl.download([video_url])

    attempts = [
        (primary_format, False),
        (fallback_format, False) if fallback_format else None,
        (primary_format, True),
        (fallback_format, True) if fallback_format else None,
    ]

    last_err = None
    for fmt, use_browser in filter(None, attempts):
        try:
            if use_browser:
                print("\nTentative avec les cookies du navigateur (Firefox)...")
            elif fmt == fallback_format:
                print("\nEchec du format principal, tentative sur un format plus simple...")
            run_download(fmt, use_browser_cookies=use_browser)
            return
        except DownloadError as err:
            last_err = err
            continue

    print(f"\nErreur lors du telechargement : {last_err}")
    if is_youtube_url(video_url) and not use_cookies:
        print("YouTube renvoie souvent 403 sans cookies recents.")
        print("Exportez vos cookies Firefox (extension 'cookies.txt') puis relancez.")
    sys.exit(1)


if __name__ == "__main__":
    default_output_path = "C:/Users/miche/Downloads"

    if len(sys.argv) > 1:
        video_url = sys.argv[1]
        output_path = sys.argv[2] if len(sys.argv) > 2 else default_output_path
        download_type = sys.argv[3].lower() if len(sys.argv) > 3 else "video"
    else:
        video_url = input("Entrez l'URL de la video YouTube a telecharger : ")
        output_path = input(f"Chemin de sauvegarde (defaut: {default_output_path}) : ") or default_output_path
        download_type = input("Type de telechargement (video/audio, defaut: video) : ").lower() or "video"

    if download_type not in ["video", "audio"]:
        print("Type de telechargement invalide. Utilisez 'video' ou 'audio'.")
        sys.exit(1)

    download_youtube_media(video_url, output_path, download_type)
    print(f"\n{'Video' if download_type == 'video' else 'Audio'} telecharge(e) avec succes !")
