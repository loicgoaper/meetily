@echo off
rem Meetily perso: build the Windows installer and copy it to <repo>\installateur,
rem ready to be copied to another computer (only the latest version is kept there).
rem Usage (from anywhere): frontend\scripts\fork-build-installer.cmd
setlocal
cd /d "%~dp0.."
call "%~dp0fork-dev-env.cmd" pnpm tauri:build:cpu
if errorlevel 1 (
  echo La compilation a echoue.
  exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$v = (Get-Content 'src-tauri\tauri.conf.json' -Raw | ConvertFrom-Json).version;" ^
  "$src = Join-Path '..\target\release\bundle\nsis' ('meetily_' + $v + '_x64-setup.exe');" ^
  "if (-not (Test-Path $src)) { Write-Error ('Installateur introuvable : ' + $src); exit 1 }" ^
  "$dst = Resolve-Path '..' | Join-Path -ChildPath 'installateur';" ^
  "New-Item -ItemType Directory -Force $dst | Out-Null;" ^
  "Get-ChildItem $dst -Filter '*.exe' | Remove-Item -Force;" ^
  "$out = Join-Path $dst ('Meetily-perso-' + $v + '-setup.exe');" ^
  "Copy-Item $src $out -Force;" ^
  "Write-Host ('Installateur pret : ' + $out)"
if errorlevel 1 exit /b 1
endlocal
