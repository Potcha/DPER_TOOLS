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


def get_js_runtime():
    """Return a supported JavaScript runtime configuration for yt-dlp."""
    for runtime in ("deno", "node"):
        runtime_path = shutil.which(runtime)
        if runtime_path:
            return {runtime: {"path": runtime_path}}
    return None


def parse_cli_arguments(arguments, default_output_path):
    """Parse URL, optional output path and optional media type."""
    if not arguments:
        return None

    video_url = arguments[0]
    output_path = default_output_path
    download_type = None

    if len(arguments) > 1:
        second_argument = arguments[1].strip()
        if second_argument.lower() in {"video", "audio", "mp3"}:
            download_type = second_argument.lower()
        else:
            output_path = second_argument

    if len(arguments) > 2:
        download_type = arguments[2].strip().lower()

    return video_url, output_path, download_type


def download_youtube_media(video_url, output_path, download_type="video"):
    """Download a YouTube video (video+audio) or audio only."""
    if not os.path.exists(output_path):
        os.makedirs(output_path)

    ffmpeg_path = shutil.which("ffmpeg")
    if ffmpeg_path is None:
        print("Erreur : ffmpeg n'est pas installe ou non accessible via la variable PATH.")
        sys.exit(1)

    js_runtimes = get_js_runtime()
    if js_runtimes is None:
        print("Erreur : aucun moteur JavaScript compatible n'est accessible dans le PATH.")
        print("Installe Deno (recommande) ou Node.js 22+, puis rouvre le terminal.")
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
        "js_runtimes": js_runtimes,
        # Autorise yt-dlp a recuperer son solveur officiel si yt-dlp-ejs
        # n'est pas installe dans l'environnement Python courant.
        "remote_components": {"ejs:github"},
        "http_headers": {
            # Laisse yt-dlp choisir un User-Agent coherent avec son client.
            "Accept-Language": "fr-FR,fr;q=0.9,en-US;q=0.8,en;q=0.7",
            "Referer": video_url,
        },
        "retries": 5,
        "fragment_retries": 5,
        "concurrent_fragment_downloads": 3,
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
    elif download_type in {"audio", "mp3"}:
        # Certaines videos proposent une piste "ase" (langue des signes) marquee
        # comme originale. Elle peut etre fortement compressee ou corrompue.
        # Privilegie une piste M4A saine, puis les autres pistes non-ASL.
        primary_format = (
            "bestaudio[ext=m4a][language=fr]/"
            "bestaudio[ext=m4a][language=en]/"
            "bestaudio[ext=m4a][language!=ase]/"
            "bestaudio[ext=webm][language=fr]/"
            "bestaudio[ext=webm][language=en]/"
            "bestaudio[ext=webm][language!=ase]/"
            "bestaudio[language!=ase]/"
            "bestaudio"
        )
        fallback_format = None
        post_opts = {
            # Le suffixe evite de reprendre un ancien .part issu d'un autre format.
            "outtmpl": os.path.join(output_path, "%(title)s [audio-mp3].%(ext)s"),
            "postprocessors": [
                {
                    "key": "FFmpegExtractAudio",
                    "preferredcodec": "mp3",
                    "preferredquality": "0",
                }
            ],
        }
        print("Mode audio : conversion directe en MP3 haute qualite.")
    else:
        print("Type de telechargement non reconnu. Utilisez 'video', 'audio' ou 'mp3'.")
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

    parsed_arguments = parse_cli_arguments(sys.argv[1:], default_output_path)
    if parsed_arguments:
        video_url, output_path, download_type = parsed_arguments
        if download_type is None:
            download_type = input("Type de telechargement (video/audio/mp3, defaut: video) : ").strip().lower() or "video"
    else:
        video_url = input("Entrez l'URL de la video YouTube a telecharger : ").strip()
        output_path = input(f"Chemin de sauvegarde (defaut: {default_output_path}) : ").strip() or default_output_path
        download_type = input("Type de telechargement (video/audio/mp3, defaut: video) : ").strip().lower() or "video"

    if download_type not in ["video", "audio", "mp3"]:
        print("Type de telechargement invalide. Utilisez 'video', 'audio' ou 'mp3'.")
        sys.exit(1)

    download_youtube_media(video_url, output_path, download_type)
    media_label = "Video" if download_type == "video" else "Audio"
    print(f"\n{media_label} telecharge(e) avec succes !")
