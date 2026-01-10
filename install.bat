@echo off
:: Run as Administrator to register .md file association

echo Installing Markdown Editor...

:: Get the directory where this script is located
set "INSTALL_DIR=%~dp0"
set "EXE_PATH=%INSTALL_DIR%dist\MarkdownEditor.exe"

:: Check if exe exists
if not exist "%EXE_PATH%" (
    echo ERROR: MarkdownEditor.exe not found in dist folder!
    echo Please build the exe first with: pyinstaller --onefile --windowed --name "MarkdownEditor" viewer.py
    pause
    exit /b 1
)

:: Create file association
echo Registering .md file association...
reg add "HKCU\Software\Classes\.md" /ve /d "MarkdownFile" /f
reg add "HKCU\Software\Classes\MarkdownFile" /ve /d "Markdown Document" /f
reg add "HKCU\Software\Classes\MarkdownFile\DefaultIcon" /ve /d "\"%EXE_PATH%\",0" /f
reg add "HKCU\Software\Classes\MarkdownFile\shell\open\command" /ve /d "\"%EXE_PATH%\" \"%%1\"" /f

:: Add "Open with Markdown Editor" to context menu
reg add "HKCU\Software\Classes\*\shell\Open with Markdown Editor\command" /ve /d "\"%EXE_PATH%\" \"%%1\"" /f

echo.
echo Installation complete!
echo - Double-click .md files to open in Markdown Editor
echo - Right-click any file and select "Open with Markdown Editor"
echo.
pause
