@echo off
cd /d "%~dp0"
echo Memex v1.2.0 启动中...
memex.exe serve --open --port 18321
pause
