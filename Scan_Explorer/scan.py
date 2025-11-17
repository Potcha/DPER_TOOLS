import os
import fnmatch
import webbrowser
from pathlib import Path
from collections import defaultdict
from datetime import datetime   # <-- ajout
import tkinter as tk
from tkinter import filedialog

# --------- Charger les patterns d'exclusion ---------
ignore_file = Path("ignore.txt")
ignore_patterns = []
if ignore_file.exists():
    with open(ignore_file, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#"):
                ignore_patterns.append(line)
else:
    ignore_patterns = [
        "CacheClip/", "OptimizedMedia/", ".gallery/", ".cache/",
        "*/Temp/", "*/tmp/", "*/Logs/", "*.tmp", "*.bak", "*.log"
    ]

def is_ignored(path: Path):
    for pattern in ignore_patterns:
        if fnmatch.fnmatch(str(path), f"*{pattern.strip()}*"):
            return True
    return False

# --------- Sélection du dossier ---------
root = tk.Tk()
root.withdraw()
folder_selected = filedialog.askdirectory(title="Choisir un dossier à analyser")
if not folder_selected:
    print("Aucun dossier sélectionné. Fin du programme.")
    exit()

root_dir = Path(folder_selected)

FILE_CATEGORIES = {
    "Images": {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tiff", ".heic"},
    "Vidéos": {".mp4", ".mov", ".avi", ".mkv", ".wmv"},
    "Audio": {".mp3", ".wav", ".flac", ".aac", ".ogg"},
    "Projets": {".drp", ".prproj", ".veg", ".aup", ".blend"},
}

def categorize_file(file: Path):
    ext = file.suffix.lower()
    for cat, exts in FILE_CATEGORIES.items():
        if ext in exts:
            return cat
    return "Autres"

total_counts = defaultdict(int)
ignored_count = 0

# ---- sous-rapports ----
OUTPUT_DIR = Path("rapports_html")
OUTPUT_DIR.mkdir(exist_ok=True)

def subreport_filename(path: Path) -> str:
    if path == root_dir:
        return "Racine.html"
    rel = path.relative_to(root_dir).as_posix()
    safe = rel.replace("/", "__")
    return f"{safe}.html"

def fmt_time(p: Path) -> str:
    try:
        return datetime.fromtimestamp(p.stat().st_mtime).strftime("%Y-%m-%d %H:%M")
    except Exception:
        return "-"

def generate_subreport(path: Path):
    out = OUTPUT_DIR / subreport_filename(path)
    try:
        items = list(path.iterdir())
    except PermissionError:
        items = []

    files = [p for p in items if p.is_file() and not is_ignored(p)]
    dirs  = [p for p in items if p.is_dir()  and not is_ignored(p)]

    # lien parent
    if path == root_dir:
        parent_link = ""
    else:
        parent = path.parent
        parent_link = f"<a href='{subreport_filename(parent)}'>📁 Up to higher level directory</a> | "

    # Chemin Windows (avec antislashs) pour un collage direct dans l’Explorateur
    windows_path = str(path)                          # ex: Q:\MEDIA\...
    windows_path_js = windows_path.replace("\\", "\\\\")  # échapper pour la string JS

    lines = [
        "<html><head><meta charset='UTF-8'><title>Contenu du dossier</title>",
        "<style>",
        "body{font-family:Arial,sans-serif;background:#1e1e1e;color:#ddd;margin:24px;}",
        "a{color:#4db2ff;text-decoration:none;} a:hover{text-decoration:underline;}",
        "h1{font-size:18px;margin:0 0 10px 0;}",
        ".bar{margin:8px 0 16px 0;display:flex;gap:10px;flex-wrap:wrap;align-items:center;}",
        "button{background:#3a3a3a;border:1px solid #555;color:#ddd;padding:6px 10px;border-radius:6px;cursor:pointer;}",
        "button:hover{background:#444;}",
        "table{width:100%;border-collapse:collapse;background:#262626;border-radius:8px;overflow:hidden;}",
        "th,td{padding:8px 10px;border-bottom:1px solid #333;}",
        "th{background:#2f2f2f;text-align:left;}",
        "</style>",
        "<script>",
        "function copyPath(p){navigator.clipboard.writeText(p).then(()=>{",
        "  const msg='Chemin copié :\\n'+p+'\\n\\nAstuce: Win+E → Ctrl+L → Ctrl+V → Entrée';",
        "  alert(msg);",
        "}).catch(()=>{alert('Impossible de copier. Sélectionne et copie manuellement :\\n'+p);});}",
        "</script>",
        "</head><body>",
        f"<h1>Index of {path.as_posix()}</h1>",
        "<div class='bar'>",
        f"{parent_link}",
        # ⬇️ Le “lien” devient un bouton qui copie le chemin (on garde le libellé demandé)
        f"<button onclick=\"copyPath('{windows_path_js}')\">📂 Ouvrir ce dossier dans l’explorateur</button>",
        # On conserve aussi (si tu veux) l’ouverture dans le navigateur local
        "</div>",
        "<table>",
        "<tr><th>Nom</th><th>Taille</th><th>Dernière modification</th></tr>"
    ]

    from datetime import datetime
    def fmt_time(p: Path) -> str:
        try:
            return datetime.fromtimestamp(p.stat().st_mtime).strftime("%Y-%m-%d %H:%M")
        except Exception:
            return "-"

    for d in sorted(dirs, key=lambda x: x.name.lower()):
        lines.append(
            f"<tr><td>📁 <a href='{subreport_filename(d)}'>{d.name}/</a></td>"
            f"<td>-</td><td>{fmt_time(d)}</td></tr>"
        )

    for f in sorted(files, key=lambda x: x.name.lower()):
        size = f"{f.stat().st_size//1024} KB"
        lines.append(
            f"<tr><td>📄 <a href='file:///{f.as_posix()}' target='_blank'>{f.name}</a></td>"
            f"<td>{size}</td><td>{fmt_time(f)}</td></tr>"
        )

    lines += ["</table></body></html>"]
    with open(out, "w", encoding="utf-8") as fp:
        fp.write("\n".join(lines))


def build_tree(path: Path, level=0):
    global ignored_count
    html = ""
    try:
        items = list(path.iterdir())
    except PermissionError:
        return ""

    files = [f for f in items if f.is_file() and not is_ignored(f)]
    dirs = [d for d in items if d.is_dir() and not is_ignored(d)]

    if not files and not dirs:
        return ""

    # Génère la page dédiée (avec lien Explorateur)
    generate_subreport(path)

    link = f"{OUTPUT_DIR.as_posix()}/{subreport_filename(path)}"
    title = '📂 Racine' if path == root_dir else f"📂 {path.name}"
    html += f"<details open class='level-{level}'><summary><a href='{link}'>{title}</a></summary>\n"

    if files:
        counts = defaultdict(int)
        for file in files:
            cat = categorize_file(file)
            counts[cat] += 1
            total_counts[cat] += 1
        html += "<ul>\n"
        for cat, count in counts.items():
            icon = '🖼️' if cat == 'Images' else '🎥' if cat == 'Vidéos' else '🎵' if cat == 'Audio' else '📄'
            html += f"<li class='file-info'>{icon} {cat} : <strong>{count}</strong></li>\n"
        html += "</ul>\n"

    for d in sorted(dirs, key=lambda x: x.name.lower()):
        subtree = build_tree(d, level + 1)
        if subtree:
            html += subtree
        else:
            ignored_count += 1

    html += "</details>\n"
    return html

# --------- Génération HTML ---------
html_content = build_tree(root_dir)

html_lines = [
    "<html><head><meta charset='UTF-8'><title>Rapport de contenu</title>",
    "<style>",
    "body{font-family:Arial,sans-serif;background:#f8f8f8;}",
    "summary{cursor:pointer;margin:5px 0;}",
    "summary a{text-decoration:none;}",
    ".level-0 summary a{font-size:1.3em;font-weight:bold;color:#004080;margin-left:0;}",
    ".level-1 summary a{font-size:1.2em;font-weight:bold;color:#0077cc;margin-left:15px;}",
    ".level-2 summary a{font-size:1.05em;font-weight:600;color:#333;margin-left:30px;}",
    ".level-3 summary a{font-size:1em;font-weight:normal;color:#555;margin-left:45px;}",
    ".level-4 summary a{font-size:0.95em;font-weight:normal;color:#777;margin-left:60px;}",
    ".file-info{color:#555;font-size:0.9em;margin-left:10px;}",
    "ul{list-style:none;margin-left:20px;padding:0;}",
    "li{margin:3px 0;}",
    "</style></head><body>",
    f"<h1>Rapport de contenu pour : {root_dir}</h1>",
    html_content,
    "<hr><h2>Résumé global</h2><ul>"
]

for cat, count in total_counts.items():
    icon = '🖼️' if cat == 'Images' else '🎥' if cat == 'Vidéos' else '🎵' if cat == 'Audio' else '📄'
    html_lines.append(f"<li>{icon} {cat} : <strong>{count}</strong></li>")

html_lines.append(f"<li>📂 Dossiers ignorés : <strong>{ignored_count}</strong></li>")
html_lines.append("</ul></body></html>")

output_file = "rapport_contenu.html"
with open(output_file, "w", encoding="utf-8") as f:
    f.write("\n".join(html_lines))

webbrowser.open_new_tab(Path(output_file).resolve().as_uri())
print(f"Rapport généré : {output_file}\nSous-rapports dans : {OUTPUT_DIR.resolve()}")
