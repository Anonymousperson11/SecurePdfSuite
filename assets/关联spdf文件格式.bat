@echo off
chcp 65001 >nul
rem 把 .spdf 文件关联到本脚本所在文件夹里的 SecurePdfReader.exe（仅对当前用户生效，不需要管理员权限）。
rem 使用安装程序并勾选了“关联 .spdf 文件后缀”时不需要再运行本脚本；它用于直接拷贝文件夹使用（免安装）的场合。
set "READER=%~dp0SecurePdfReader.exe"
if not exist "%READER%" (
    echo 未找到 SecurePdfReader.exe，请把本脚本放在阅读端程序所在的文件夹内再运行。
    pause
    exit /b 1
)
reg add "HKCU\Software\Classes\.spdf" /ve /t REG_SZ /d "SecurePdfSuite.SpdfFile" /f >nul
reg add "HKCU\Software\Classes\SecurePdfSuite.SpdfFile" /ve /t REG_SZ /d "SecurePdf 加密保护文档" /f >nul
reg add "HKCU\Software\Classes\SecurePdfSuite.SpdfFile\DefaultIcon" /ve /t REG_SZ /d "\"%READER%\",0" /f >nul
reg add "HKCU\Software\Classes\SecurePdfSuite.SpdfFile\shell\open\command" /ve /t REG_SZ /d "\"%READER%\" \"%%1\"" /f >nul
reg delete "HKCU\Software\Classes\SecurePdfSuite.spdf" /f >nul 2>&1
echo 已将 .spdf 文件关联到 %READER%
pause
