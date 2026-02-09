#Requires -RunAsAdministrator
$dest = "C:\Program Files\Git\usr\share\perl5\vendor_perl\Locale\Maketext"
New-Item -ItemType Directory -Path $dest -Force | Out-Null
Copy-Item "D:\App\Fulldesk\.perl_lib\Locale\Maketext\Simple.pm" "$dest\Simple.pm" -Force
Write-Host "Installed Locale::Maketext::Simple to $dest" -ForegroundColor Green
