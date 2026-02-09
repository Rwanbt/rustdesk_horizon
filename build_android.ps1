# build_android.ps1
# Script de compilation automatisé pour l'APK Android (Fulldesk)

$ErrorActionPreference = "Stop"
$ProjectRoot = $PSScriptRoot

Write-Host "=== Fulldesk Android Build Script ===" -ForegroundColor Cyan

# --- Étape 0 : Détection de l'environnement ---

# 1. Flutter
$FlutterPath = "C:\flutter\bin\flutter.bat"
if (-not (Test-Path $FlutterPath)) {
    $FlutterPath = "flutter" # Tentative via PATH si pas au chemin par défaut
}
Write-Host "[0/4] Vérification de Flutter..." -ForegroundColor Yellow
& $FlutterPath --version | Out-Null
Write-Host "  Flutter OK" -ForegroundColor Green

# 2. Android SDK & NDK
$SdkPath = "$env:LOCALAPPDATA\Android\Sdk"
if (-not (Test-Path $SdkPath)) {
    throw "Android SDK non trouvé à $SdkPath. Veuillez adapter le script ou installer Android Studio."
}

$NdkRoot = Join-Path $SdkPath "ndk"
if (-not (Test-Path $NdkRoot)) {
    throw "Android NDK non trouvé dans le SDK. Installez 'NDK (Side by side)' via Android Studio."
}

$NdkDir = Get-ChildItem -Path $NdkRoot | Sort-Object Name -Descending | Select-Object -First 1
if (-not $NdkDir) {
    throw "Aucune version du NDK trouvée dans $NdkRoot"
}
$env:ANDROID_NDK_HOME = $NdkDir.FullName
Write-Host "  Android NDK OK ($($NdkDir.Name))" -ForegroundColor Green

# 3. Rust Targets
Write-Host "  Vérification de la cible Rust..." -ForegroundColor Yellow
$Targets = rustup target list --installed
if ($Targets -notcontains "aarch64-linux-android") {
    Write-Host "  Installation de la cible aarch64-linux-android..."
    rustup target add aarch64-linux-android
}

# 4. cargo-ndk
if (-not (Get-Command "cargo-ndk" -ErrorAction SilentlyContinue)) {
    Write-Host "  Installation de cargo-ndk..." -ForegroundColor Yellow
    cargo install cargo-ndk
}

# --- Étape 1 : vcpkg pour Android ---
Write-Host "`n[1/4] Installation des dépendances vcpkg (arm64-android)..." -ForegroundColor Yellow
$env:VCPKG_ROOT = "C:\vcpkg"
if (Test-Path "$env:VCPKG_ROOT\vcpkg.exe") {
    & "$env:VCPKG_ROOT\vcpkg.exe" install --triplet arm64-android --x-install-root="$env:VCPKG_ROOT\installed"
}
else {
    Write-Host "  WARNING: vcpkg non trouvé. La compilation Rust pourrait échouer si les libs ne sont pas en cache." -ForegroundColor Red
}

# --- Étape 2 : Compilation Rust (.so) ---
Write-Host "`n[2/4] Compilation du moteur Rust (aarch64)..." -ForegroundColor Yellow
# On s'assure d'être à la racine du projet
Push-Location $ProjectRoot
try {
    cargo ndk --platform 21 --target aarch64-linux-android build --release --features flutter, hwcodec
}
finally {
    Pop-Location
}

# --- Étape 3 : Placement de la librairie ---
Write-Host "`n[3/4] Préparation du dossier jniLibs..." -ForegroundColor Yellow
$jniDir = Join-Path $ProjectRoot "flutter\android\app\src\main\jniLibs\arm64-v8a"
if (-not (Test-Path $jniDir)) {
    New-Item -ItemType Directory -Path $jniDir -Force | Out-Null
}
Copy-Item "$ProjectRoot\target\aarch64-linux-android\release\liblibrustdesk.so" -Destination "$jniDir\librustdesk.so" -Force
Write-Host "  Library copied to jniLibs" -ForegroundColor Green

# --- Étape 4 : Compilation Flutter APK ---
Write-Host "`n[4/4] Génération de l'APK finale..." -ForegroundColor Yellow
Push-Location "$ProjectRoot\flutter"
try {
    & $FlutterPath build apk --release --target-platform android-arm64
}
finally {
    Pop-Location
}

Write-Host "`n=== Build terminé avec SUCCÈS ! ===" -ForegroundColor Cyan
Write-Host "L'APK se trouve ici : $ProjectRoot\flutter\build\app\outputs\flutter-apk\app-release.apk" -ForegroundColor Green
