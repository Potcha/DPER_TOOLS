@echo off
cd /d "%~dp0"
python -m pip install -r requirements.txt
python import_yt_dlp.py --help
pause
