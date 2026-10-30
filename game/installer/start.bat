@echo off
setlocal
cd /d "%~dp0"
set "PATH=%~dp0tools;%PATH%"
if "%HTTP_PORT%"=="" set "HTTP_PORT=4002"

rem R4_DEMO=1      : enable R4 AI seats
rem R4_DEMO_PAUSE=1: no auto 4-AI table; humans sit in and AI fills the rest.
rem                  Delete the next line to get an auto-running 4-AI table.
set "R4_DEMO=1"
set "R4_DEMO_PAUSE=1"

if not exist "%~dp0models\r4_gen2.pth" (echo missing models\r4_gen2.pth & pause & exit /b 1)
if not exist "%~dp0ai\python\python.exe" (echo missing ai\python & pause & exit /b 1)

netstat -ano | findstr /R /C:":%HTTP_PORT% .*LISTENING" >nul && (
  echo Port %HTTP_PORT% is already in use. Run stop.bat or close the old server first.
  pause
  exit /b 1
)
netstat -ano | findstr /R /C:":8471 .*LISTENING" >nul && (
  echo Port 8471 is already in use. Run stop.bat or close the old AI engine first.
  pause
  exit /b 1
)

start "Z-Mortal AI" /min "%~dp0ai\python\python.exe" "%~dp0ai\r4_service.py"
echo starting AI engine (first launch takes ~15s)...
timeout /t 6 /nobreak >nul
pushd "%~dp0server"
start "Z-Mortal Server" /min bin\riichi_advanced.bat start
popd
echo starting server on port %HTTP_PORT% ...
timeout /t 12 /nobreak >nul
start "" "http://localhost:%HTTP_PORT%/"
echo.
echo Z-Mortal is up. Close the two minimized windows (Z-Mortal AI / Z-Mortal Server) to quit,
echo or run stop.bat.
endlocal
