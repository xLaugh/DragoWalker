# DragoWalker : prépare la version à distribuer.
#   powershell -ExecutionPolicy Bypass -File tools\package.ps1
# Résultat : ..\dist\DragoWalker-<version>\DragoWalker.exe (+ LISEZMOI.txt) et ..\dist\DragoWalker-<version>.zip
$ErrorActionPreference = 'Stop'
$app = Split-Path $PSScriptRoot -Parent
Set-Location $app

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"').Matches[0].Groups[1].Value

# Runtime Visual C++ intégré à l'exe : rien à installer chez l'utilisateur.
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "Le build release a échoué." }

$dist = Join-Path (Split-Path $app -Parent) 'dist'
$out = Join-Path $dist "DragoWalker-$version"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item target\release\dragowalker.exe (Join-Path $out 'DragoWalker.exe')

@"
DragoWalker $version - éditeur de cartes GTA V

INSTALLATION
Aucune. Lancez DragoWalker.exe (version portable).
Les réglages sont enregistrés dans %APPDATA%\DragoWalker.

NÉCESSAIRE
- Windows 10 ou 11, 64 bits
- Une carte graphique compatible OpenGL 3.1 ou plus
- GTA V installé (version PC)
- CodeWalker (indiquez son dossier dans le panneau « Jeu » au premier lancement)

OPTIONNEL
- Blender + Sollumz : modèles 3D
- Claude Code, une clé API Claude ou un serveur compatible OpenAI : assistant IA

Windows peut afficher « Windows a protégé votre PC » : cliquez sur
« Informations complémentaires » puis « Exécuter quand même ».
"@ | Out-File -Encoding utf8 (Join-Path $out 'LISEZMOI.txt')

$zip = Join-Path $dist "DragoWalker-$version.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $out -DestinationPath $zip
Write-Host "Portable : $zip"

# Installeur (Inno Setup 6, https://jrsoftware.org/isinfo.php)
$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    Write-Warning "Inno Setup 6 introuvable : installeur non créé (seule la version portable est prête)."
    return
}
& $iscc /Q "/DAppVersion=$version" "/DSrcDir=$out" (Join-Path $PSScriptRoot 'installer.iss')
if ($LASTEXITCODE -ne 0) { throw "La création de l'installeur a échoué." }
Write-Host "Installeur : $(Join-Path $dist "DragoWalker-$version-setup.exe")"

# Empreintes SHA-256 des fichiers à publier (l'app les vérifie avant d'installer une mise à jour)
$setup = Join-Path $dist "DragoWalker-$version-setup.exe"
$lines = foreach ($f in @($setup, $zip)) { if (Test-Path $f) { "{0}  {1}" -f (Get-FileHash $f -Algorithm SHA256).Hash.ToLower(), (Split-Path $f -Leaf) } }
$lines | Out-File -Encoding ascii (Join-Path $dist 'SHA256SUMS.txt')
Write-Host "Empreintes : $(Join-Path $dist 'SHA256SUMS.txt')"
