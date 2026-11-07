@echo off
chcp 65001 >nul 2>&1
title Memex 启动器
cd /d "%~dp0"

:: Check if already running
tasklist /FI "IMAGENAME eq memex.exe" 2>nul | find /I "memex.exe" >nul
if %ERRORLEVEL%==0 goto already_running

echo.
echo  ========================================
echo           Memex 启动方式选择
echo  ========================================
echo.
echo  1. 后台模式（推荐）
echo     内存约 15MB，仅系统托盘图标
echo     不开窗口，配合 AI 助手搜索文档
echo     左键点托盘图标可打开浏览器界面
echo.
echo  2. 窗口模式
echo     内存约 390MB，显示完整管理界面
echo     可查看搜索结果、统计图表和设置
echo     关闭窗口自动隐藏到托盘，不退出
echo.
echo  ========================================
echo.
choice /c 12 /n /m "请选择启动方式（输入 1 或 2）"

if errorlevel 2 goto window
if errorlevel 1 goto tray

:tray
echo.
echo  正在以后台模式启动 Memex...
start "" memex.exe serve --no-window
echo  已启动！托盘区可看到 Memex 图标。
echo  右键托盘图标可选择"退出"。
timeout /t 3 /nobreak >nul
exit

:window
echo.
echo  正在启动 Memex 窗口...
start "" memex.exe serve
exit

:already_running
echo.
echo  ========================================
echo         Memex 已在运行中
echo  ========================================
echo.
echo  Memex 正在后台运行。
echo  如需停止，请右键托盘图标选择"退出"。
echo.
echo  1. 打开浏览器界面
echo  2. 退出此启动器（不做任何操作）
echo.
choice /c 12 /n /m "请选择（输入 1 或 2）"

if errorlevel 2 exit
if errorlevel 1 (
  start "" http://127.0.0.1:18321
  exit
)
