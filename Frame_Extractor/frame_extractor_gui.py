#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Frame Extractor GUI (with vertical scrollbar)
---------------------------------------------
Une petite interface Tkinter pour extraire chaque image (frame) d'une vidéo.
Dépendances:
  - opencv-python
  - pillow

Installation (exemples):
  pip install opencv-python pillow

Usage:
  python frame_extractor_gui.py
"""
import os
import threading
import time
import tkinter as tk
from tkinter import ttk, filedialog, messagebox
from dataclasses import dataclass

# Lazy import pour accélérer le lancement et afficher des messages d'erreur clairs
try:
    import cv2
except Exception as e:
    cv2 = None
    _cv2_import_error = e
else:
    _cv2_import_error = None

try:
    from PIL import Image, ImageTk
except Exception as e:
    Image = None
    ImageTk = None
    _pillow_import_error = e
else:
    _pillow_import_error = None


@dataclass
class Settings:
    video_path: str = ""
    output_dir: str = ""
    fmt: str = "png"              # "png" ou "jpg"
    prefix: str = "frame_"
    zpad: int = 6
    start_index: int = 1
    every_n: int = 1              # 1 = chaque frame, 2 = 1 frame sur 2, etc.
    jpeg_quality: int = 95        # pour JPG uniquement


class ScrollableContent(ttk.Frame):
    """Cadre avec scroll vertical (Canvas + Frame interne)"""
    def __init__(self, master, *args, **kwargs):
        super().__init__(master, *args, **kwargs)
        self.canvas = tk.Canvas(self, highlightthickness=0)
        self.vscroll = ttk.Scrollbar(self, orient="vertical", command=self.canvas.yview)
        self.canvas.configure(yscrollcommand=self.vscroll.set)

        self.inner = ttk.Frame(self.canvas)
        self.inner_id = self.canvas.create_window((0, 0), window=self.inner, anchor="nw")

        self.canvas.grid(row=0, column=0, sticky="nsew")
        self.vscroll.grid(row=0, column=1, sticky="ns")
        self.grid_rowconfigure(0, weight=1)
        self.grid_columnconfigure(0, weight=1)

        # Redimensionner la largeur du cadre interne pour suivre le canvas
        self.inner.bind("<Configure>", self._on_inner_configure)
        self.canvas.bind("<Configure>", self._on_canvas_configure)

        # Binding molette (Windows/Mac/Linux)
        self._bind_mousewheel(self.canvas)
        self._bind_mousewheel(self.inner)

    def _on_inner_configure(self, event):
        # Mettre à jour la scrollregion et garder la largeur synchrone
        self.canvas.configure(scrollregion=self.canvas.bbox("all"))
        self.canvas.itemconfig(self.inner_id, width=self.canvas.winfo_width())

    def _on_canvas_configure(self, event):
        # Ajuster la largeur du frame intérieur quand on redimensionne la fenêtre
        self.canvas.itemconfig(self.inner_id, width=event.width)

    def _bind_mousewheel(self, widget):
        # Windows / MacOS / Linux
        widget.bind_all("<MouseWheel>", self._on_mousewheel, add="+")      # Windows / Mac
        widget.bind_all("<Shift-MouseWheel>", self._on_shift_mousewheel, add="+")
        widget.bind_all("<Button-4>", self._on_mousewheel_linux, add="+")  # Linux
        widget.bind_all("<Button-5>", self._on_mousewheel_linux, add="+")

    def _on_mousewheel(self, event):
        delta = int(-1 * (event.delta / 120))
        self.canvas.yview_scroll(delta, "units")

    def _on_shift_mousewheel(self, event):
        # scroll horizontal si besoin (non utilisé ici)
        pass

    def _on_mousewheel_linux(self, event):
        delta = -1 if event.num == 5 else 1
        self.canvas.yview_scroll(delta, "units")


class FrameExtractorGUI(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("Extraction d'images d'une vidéo")
        self.geometry("780x560")
        self.minsize(700, 480)

        # État
        self.settings = Settings()
        self.cap = None
        self.total_frames = 0
        self.fps = 0.0
        self.duration_s = 0.0
        self.worker = None
        self.stop_flag = threading.Event()

        self._build_ui()

    def _build_ui(self):
        pad = 8

        # ====== Layout principal: zone scrollable + barre de progression fixe en bas ======
        self.columnconfigure(0, weight=1)
        self.rowconfigure(0, weight=1)

        self.scroll_area = ScrollableContent(self)
        self.scroll_area.grid(row=0, column=0, sticky="nsew")

        content = self.scroll_area.inner  # Le frame intérieur scrollable

        # Cadre sélection fichier/dossier
        file_frame = ttk.LabelFrame(content, text="Fichiers")
        file_frame.pack(fill="x", padx=pad, pady=(pad, 4))

        self.video_var = tk.StringVar()
        self.outdir_var = tk.StringVar()

        ttk.Label(file_frame, text="Vidéo :").grid(row=0, column=0, sticky="w", padx=pad, pady=pad)
        ttk.Entry(file_frame, textvariable=self.video_var, width=70).grid(row=0, column=1, sticky="we", padx=(0, pad), pady=pad)
        ttk.Button(file_frame, text="Choisir…", command=self.choose_video).grid(row=0, column=2, padx=(0, pad), pady=pad)

        ttk.Label(file_frame, text="Dossier de sortie :").grid(row=1, column=0, sticky="w", padx=pad, pady=(0, pad))
        ttk.Entry(file_frame, textvariable=self.outdir_var, width=70).grid(row=1, column=1, sticky="we", padx=(0, pad), pady=(0, pad))
        ttk.Button(file_frame, text="Parcourir…", command=self.choose_outdir).grid(row=1, column=2, padx=(0, pad), pady=(0, pad))

        file_frame.columnconfigure(1, weight=1)

        # Cadre options
        opt_frame = ttk.LabelFrame(content, text="Options d'export")
        opt_frame.pack(fill="x", padx=pad, pady=4)

        self.fmt_var = tk.StringVar(value=self.settings.fmt)
        ttk.Label(opt_frame, text="Format :").grid(row=0, column=0, sticky="w", padx=pad, pady=pad)
        fmt_cb = ttk.Combobox(opt_frame, textvariable=self.fmt_var, values=["png", "jpg"], width=6, state="readonly")
        fmt_cb.grid(row=0, column=1, sticky="w", padx=(0, pad), pady=pad)

        self.prefix_var = tk.StringVar(value=self.settings.prefix)
        ttk.Label(opt_frame, text="Préfixe fichier :").grid(row=0, column=2, sticky="w", padx=pad, pady=pad)
        ttk.Entry(opt_frame, textvariable=self.prefix_var, width=18).grid(row=0, column=3, sticky="w", padx=(0, pad), pady=pad)

        self.zpad_var = tk.IntVar(value=self.settings.zpad)
        ttk.Label(opt_frame, text="Zero-padding :").grid(row=0, column=4, sticky="w", padx=pad, pady=pad)
        ttk.Spinbox(opt_frame, from_=1, to=12, textvariable=self.zpad_var, width=5).grid(row=0, column=5, sticky="w", padx=(0, pad), pady=pad)

        self.start_idx_var = tk.IntVar(value=self.settings.start_index)
        ttk.Label(opt_frame, text="Index départ :").grid(row=1, column=0, sticky="w", padx=pad, pady=(0, pad))
        ttk.Spinbox(opt_frame, from_=0, to=10_000_000, textvariable=self.start_idx_var, width=8).grid(row=1, column=1, sticky="w", padx=(0, pad), pady=(0, pad))

        self.every_n_var = tk.IntVar(value=self.settings.every_n)
        ttk.Label(opt_frame, text="1 frame sur N :").grid(row=1, column=2, sticky="w", padx=pad, pady=(0, pad))
        ttk.Spinbox(opt_frame, from_=1, to=1000, textvariable=self.every_n_var, width=8).grid(row=1, column=3, sticky="w", padx=(0, pad), pady=(0, pad))

        self.quality_var = tk.IntVar(value=self.settings.jpeg_quality)
        self.quality_label = ttk.Label(opt_frame, text="Qualité JPG :")
        self.quality_entry = ttk.Spinbox(opt_frame, from_=50, to=100, textvariable=self.quality_var, width=8)
        self.quality_label.grid(row=1, column=4, sticky="w", padx=pad, pady=(0, pad))
        self.quality_entry.grid(row=1, column=5, sticky="w", padx=(0, pad), pady=(0, pad))

        opt_frame.columnconfigure(3, weight=1)

        # Cadre info & preview
        info_frame = ttk.LabelFrame(content, text="Infos vidéo & aperçu")
        info_frame.pack(fill="both", expand=True, padx=pad, pady=4)

        self.info_text = tk.StringVar(value="Aucune vidéo chargée.")
        ttk.Label(info_frame, textvariable=self.info_text, anchor="w", justify="left").pack(fill="x", padx=pad, pady=pad)

        self.preview_label = ttk.Label(info_frame, anchor="center")
        self.preview_label.pack(fill="both", expand=True, padx=pad, pady=(0, pad))

        # ====== Barre inférieure fixe (progress + commandes) ======
        bottom = ttk.Frame(self)
        bottom.grid(row=1, column=0, sticky="ew", padx=pad, pady=(0, pad))
        bottom.columnconfigure(0, weight=1)

        self.progress = ttk.Progressbar(bottom, mode="determinate")
        self.progress.grid(row=0, column=0, sticky="ew", padx=(0, pad))

        self.start_btn = ttk.Button(bottom, text="Démarrer", command=self.start_extraction)
        self.start_btn.grid(row=0, column=1, padx=(0, pad))

        self.stop_btn = ttk.Button(bottom, text="Stop", command=self.stop_extraction, state="disabled")
        self.stop_btn.grid(row=0, column=2)

        # Lier évènements
        fmt_cb.bind("<<ComboboxSelected>>", self.on_fmt_changed)

    def on_fmt_changed(self, *_):
        is_jpg = (self.fmt_var.get().lower() == "jpg")
        state = "normal" if is_jpg else "disabled"
        self.quality_label.configure(state=state)
        self.quality_entry.configure(state=state)

    def choose_video(self):
        path = filedialog.askopenfilename(
            title="Choisir une vidéo",
            filetypes=[
                ("Vidéos", "*.mp4;*.mov;*.avi;*.mkv;*.m4v;*.webm"),
                ("Tous les fichiers", "*.*"),
            ]
        )
        if path:
            self.video_var.set(path)
            self.load_video_info(path)

    def choose_outdir(self):
        out = filedialog.askdirectory(title="Choisir le dossier de sortie")
        if out:
            self.outdir_var.set(out)

    def load_video_info(self, path):
        if cv2 is None:
            messagebox.showerror("OpenCV manquant",
                                 f"Le module opencv-python n'est pas installé.\nErreur: {_cv2_import_error}")
            return
        try:
            cap = cv2.VideoCapture(path)
            if not cap.isOpened():
                raise RuntimeError("Impossible d'ouvrir la vidéo.")
            total = int(cap.get(cv2.CAP_PROP_FRAME_COUNT) or 0)
            fps = float(cap.get(cv2.CAP_PROP_FPS) or 0.0)
            w = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH) or 0)
            h = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT) or 0)
            duration = (total / fps) if (total > 0 and fps > 0) else 0.0

            self.total_frames = total
            self.fps = fps
            self.duration_s = duration

            info = [
                f"Fichier : {os.path.basename(path)}",
                f"Résolution : {w} x {h}",
                f"FPS : {fps:.3f}" if fps else "FPS : (inconnu)",
                f"Nombre de frames : {total}" if total else "Nombre de frames : (inconnu)",
                f"Durée : {duration:.2f} s" if duration else "Durée : (inconnue)",
            ]
            self.info_text.set("\n".join(info))

            # aperçu première frame
            ret, frame = cap.read()
            if ret and Image is not None and ImageTk is not None:
                self.show_preview(frame)
            cap.release()
        except Exception as e:
            messagebox.showerror("Erreur vidéo", f"Échec de lecture des infos vidéo.\n{e}")

    def show_preview(self, frame_bgr):
        # Convertir BGR -> RGB et redimensionner à la zone de preview
        h = max(1, self.preview_label.winfo_height())
        w = max(1, self.preview_label.winfo_width())
        # Si taille widget pas encore dispo, on choisit une taille max
        if w < 50 or h < 50:
            max_w, max_h = 640, 360
        else:
            max_w, max_h = w, h

        rgb = cv2.cvtColor(frame_bgr, cv2.COLOR_BGR2RGB)
        img = Image.fromarray(rgb)
        img.thumbnail((max_w, max_h))
        self._preview_imgtk = ImageTk.PhotoImage(img)
        self.preview_label.configure(image=self._preview_imgtk)

    def validate_inputs(self):
        if not self.video_var.get():
            messagebox.showwarning("Vidéo manquante", "Merci de choisir un fichier vidéo.")
            return False
        if not os.path.isfile(self.video_var.get()):
            messagebox.showerror("Fichier introuvable", "Le fichier vidéo n'existe pas.")
            return False
        outdir = self.outdir_var.get() or ""
        if not outdir:
            messagebox.showwarning("Dossier de sortie", "Merci de choisir un dossier de sortie.")
            return False
        try:
            os.makedirs(outdir, exist_ok=True)
        except Exception as e:
            messagebox.showerror("Dossier invalide", f"Impossible de créer/écrire dans le dossier.\n{e}")
            return False
        if cv2 is None:
            messagebox.showerror("OpenCV manquant",
                                 f"Le module opencv-python n'est pas installé.\nErreur: {_cv2_import_error}")
            return False
        if (Image is None) or (ImageTk is None):
            messagebox.showwarning("Pillow manquant",
                                   f"Pillow (PIL) n'est pas installé. L'aperçu ne sera pas affiché.\nErreur: {_pillow_import_error}")
        return True

    def start_extraction(self):
        if not self.validate_inputs():
            return

        # Remplir settings
        self.settings.video_path = self.video_var.get()
        self.settings.output_dir = self.outdir_var.get()
        self.settings.fmt = self.fmt_var.get().lower()
        self.settings.prefix = self.prefix_var.get()
        self.settings.zpad = int(self.zpad_var.get())
        self.settings.start_index = int(self.start_idx_var.get())
        self.settings.every_n = max(1, int(self.every_n_var.get()))
        self.settings.jpeg_quality = int(self.quality_var.get())

        self.stop_flag.clear()
        self.start_btn.configure(state="disabled")
        self.stop_btn.configure(state="normal")

        # Init barre de progression
        if self.total_frames > 0 and self.settings.every_n >= 1:
            steps = (self.total_frames + (self.settings.every_n - 1)) // self.settings.every_n
            self.progress.configure(mode="determinate", maximum=max(1, steps), value=0)
        else:
            self.progress.configure(mode="indeterminate")
            self.progress.start(10)

        self.worker = threading.Thread(target=self._extract_worker, daemon=True)
        self.worker.start()
        self.after(200, self._poll_worker)

    def stop_extraction(self):
        self.stop_flag.set()

    def _poll_worker(self):
        if self.worker and self.worker.is_alive():
            self.after(200, self._poll_worker)
        else:
            self.start_btn.configure(state="normal")
            self.stop_btn.configure(state="disabled")
            try:
                self.progress.stop()
            except Exception:
                pass

    def _extract_worker(self):
        s = self.settings
        try:
            cap = cv2.VideoCapture(s.video_path)
            if not cap.isOpened():
                raise RuntimeError("Impossible d'ouvrir la vidéo.")

            # Paramètres d'encodage pour JPG (cv2.imwrite -> paramètre IMWRITE_JPEG_QUALITY)
            jpg_params = [int(cv2.IMWRITE_JPEG_QUALITY), max(50, min(100, s.jpeg_quality))]

            frame_idx = 0
            saved_idx = s.start_index
            last_preview_time = 0.0

            while not self.stop_flag.is_set():
                ret, frame = cap.read()
                if not ret:
                    break

                # Traiter 1 frame sur N
                if (frame_idx % s.every_n) == 0:
                    fname = f"{s.prefix}{str(saved_idx).zfill(s.zpad)}.{s.fmt}"
                    fpath = os.path.join(s.output_dir, fname)
                    if s.fmt == "jpg":
                        ok = cv2.imwrite(fpath, frame, jpg_params)
                    else:
                        ok = cv2.imwrite(fpath, frame)

                    if not ok:
                        raise RuntimeError(f"Échec d'écriture: {fpath}")
                    saved_idx += 1

                    # Mise à jour preview (pas trop souvent pour ne pas ralentir)
                    now = time.time()
                    if (now - last_preview_time) > 0.05 and Image is not None and ImageTk is not None:
                        self.after(0, self.show_preview, frame.copy())
                        last_preview_time = now

                    # Progression
                    if self.progress["mode"] == "determinate":
                        self.after(0, self.progress.step, 1)

                frame_idx += 1

            cap.release()
            if self.stop_flag.is_set():
                self.after(0, lambda: messagebox.showinfo("Interrompu", "Extraction interrompue par l'utilisateur."))
            else:
                self.after(0, lambda: messagebox.showinfo("Terminé", "Extraction terminée avec succès."))

        except Exception as e:
            self.after(0, lambda: messagebox.showerror("Erreur", str(e)))
        finally:
            self.after(0, lambda: (self.start_btn.configure(state="normal"),
                                   self.stop_btn.configure(state="disabled")))
            try:
                self.after(0, self.progress.stop)
            except Exception:
                pass


def main():
    app = FrameExtractorGUI()
    app.on_fmt_changed()
    app.mainloop()


if __name__ == "__main__":
    main()
