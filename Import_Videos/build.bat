@echo off
cd /d "%~dp0"
python -m pip install -r requirements.txt
python -m pip install pyinstaller
pyinstaller --onefile --console import_yt_dlp.py -n import-videos
echo.
echo Build termine. Binaire : DPER_TOOLS\Import_Videos\dist\import-videos.exe
pause
