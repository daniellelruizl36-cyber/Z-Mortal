@echo off
setlocal
cd /d "%~dp0"
taskkill /F /FI "WINDOWTITLE eq Z-Mortal AI*" >nul 2>&1
taskkill /F /FI "WINDOWTITLE eq Z-Mortal Server*" >nul 2>&1
pushd "%~dp0server"
cmd /c bin\riichi_advanced.bat stop >nul 2>&1
popd
echo stopped.
endlocal
