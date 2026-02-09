@echo off
echo === Installation du serveur Fulldesk (RustDesk IDD) ===
echo.

echo [1/6] Arret du service RustDesk...
net stop RustDesk
taskkill /F /IM rustdesk.exe 2>nul
timeout /t 2 /nobreak >nul

echo [2/6] Copie de librustdesk.dll...
copy /Y "D:\App\Fulldesk\target\release\librustdesk.dll" "C:\Program Files\RustDesk\librustdesk.dll"
if %errorlevel% neq 0 (
    echo ERREUR: Copie librustdesk.dll echouee!
    pause
    exit /b 1
)

echo [3/6] Copie de dylib_virtual_display.dll...
copy /Y "D:\App\Fulldesk\target\release\dylib_virtual_display.dll" "C:\Program Files\RustDesk\dylib_virtual_display.dll"
if %errorlevel% neq 0 (
    echo ERREUR: Copie dylib_virtual_display.dll echouee!
    pause
    exit /b 1
)

echo [4/6] Installation des fichiers du driver RustDesk IDD...
if not exist "C:\Program Files\RustDesk\RustDeskIddDriver" mkdir "C:\Program Files\RustDesk\RustDeskIddDriver"
xcopy /Y /E "D:\App\Fulldesk\RustDeskIddDriver\RustDeskIddDriver\*" "C:\Program Files\RustDesk\RustDeskIddDriver\"
if %errorlevel% neq 0 (
    echo ERREUR: Copie driver echouee!
    pause
    exit /b 1
)

echo [5/6] Installation du certificat RustDesk IDD...
certutil -addstore -f "TrustedPublisher" "D:\App\Fulldesk\RustDeskIddDriver\RustDeskIddDriver.cer"
if %errorlevel% neq 0 (
    echo ATTENTION: Installation du certificat echouee, le driver pourrait ne pas s'installer.
)

echo [6/6] Redemarrage de RustDesk...
net start RustDesk
start "" "C:\Program Files\RustDesk\rustdesk.exe"

echo.
echo === Installation terminee! ===
echo Fichiers installes:
echo   - librustdesk.dll (backend Rust)
echo   - dylib_virtual_display.dll (interface driver IDD)
echo   - RustDeskIddDriver/ (kernel driver)
echo   - Certificat TrustedPublisher
pause
