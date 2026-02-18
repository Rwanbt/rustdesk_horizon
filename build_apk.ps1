$env:JAVA_HOME = "C:\Program Files\Android\Android Studio\jbr"
$env:PATH = "C:\flutter\bin;" + $env:PATH
Set-Location "d:\App\Fulldesk\flutter"
Write-Host "Running flutter pub get..."
& flutter pub get
Write-Host "Building APK..."
& flutter build apk --debug --target-platform android-arm64
