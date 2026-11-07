@echo off
cd /d "%~dp0"
echo Memex v1.0.0 启动中...
memex.exe serve --open --port 18321
pause
