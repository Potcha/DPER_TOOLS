@echo off
setlocal
cd /d "%~dp0"
python -V || (echo Python introuvable & pause & exit /b 1)
pip show pyinstaller >nul 2>&1 || pip install pyinstaller
pyinstaller --onefile --windowed --clean scan_explorer.py -n scan-explorer
echo.
echo Build termine. Binaire : tools\scan-explorer\dist\scan-explorer.exe
pause
