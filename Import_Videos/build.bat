@echo off
cd /d "%~dp0"
python -m pip install -r requirements.txt || goto :error
python -m pip install pyinstaller || goto :error
python -m PyInstaller --clean --noconfirm --onefile --console --icon "Youtube_3D.ico" import_yt_dlp.py -n import-videos || goto :error
echo.
echo Build termine. Binaire : DPER_TOOLS\Import_Videos\dist\import-videos.exe
pause
goto :eof

:error
echo.
echo Build en echec.
pause
exit /b 1
