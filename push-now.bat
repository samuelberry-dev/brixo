@echo off
cd /d "%~dp0"
findstr /x "push-log.txt" .git\info\exclude >/dev/null 2>&1 || echo push-log.txt>>.git\info\exclude
findstr /x "push-now.bat" .git\info\exclude >/dev/null 2>&1 || echo push-now.bat>>.git\info\exclude
echo Pushing Brixo changes...
(
  git status --short
  git add -A
  git commit -m "Steadier prediction pacing, instant tool swings"
  git push
  echo EXIT %ERRORLEVEL%
  git log --oneline -3
  git status -sb
) > push-log.txt 2>&1
type push-log.txt
echo.
echo Done. This window closes in 20 seconds.
timeout /t 20 >nul
