@echo off
cd /d c:\Users\HanTY\Desktop\rust\Graphite\graphite-host
set LOG=c:\Users\HanTY\Desktop\rust\Graphite\crates\gradle_log.txt
del /q "%LOG%" 2>nul

echo [Graphite] Запуск Gradle. Лог: %LOG%
call gradlew.bat prepareRun runClient > "%LOG%" 2>&1

echo.
echo [Graphite] Gradle завершился с кодом %errorlevel%
echo [Graphite] Последние строки лога:
type "%LOG%" | more +0

pause
