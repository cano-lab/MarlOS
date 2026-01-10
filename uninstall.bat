@echo off
:: Uninstall Markdown Editor file associations

echo Uninstalling Markdown Editor...

reg delete "HKCU\Software\Classes\.md" /f 2>nul
reg delete "HKCU\Software\Classes\MarkdownFile" /f 2>nul
reg delete "HKCU\Software\Classes\*\shell\Open with Markdown Editor" /f 2>nul
reg delete "HKCU\Software\Classes\*\shell\Open with Markdown Viewer" /f 2>nul

echo.
echo Uninstall complete!
pause
