@echo off
echo === Installation du serveur Fulldesk ===
echo.

echo [1/3] Arret du service RustDesk...
net stop RustDesk
taskkill /F /IM rustdesk.exe 2>nul
timeout /t 2 /nobreak >nul

echo [2/3] Copie de librustdesk.dll...
copy /Y "D:\App\Fulldesk\target\release\librustdesk.dll" "C:\Program Files\RustDesk\librustdesk.dll"
if %errorlevel% neq 0 (
    echo ERREUR: Copie echouee! Verifiez que RustDesk est bien arrete.
    pause
    exit /b 1
)

echo [3/3] Redemarrage de RustDesk...
net start RustDesk
start "" "C:\Program Files\RustDesk\rustdesk.exe"

echo.
echo === Installation terminee! ===
pause
