@echo off
python "%~dp0emcc-linker.py" %*
exit /b %ERRORLEVEL%
