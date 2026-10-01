@echo off
setlocal

for /f "usebackq delims=" %%i in (`wsl.exe wslpath -u "%~dp0..\ansible"`) do set "ansible_dir=%%i"
wsl.exe bash "%ansible_dir%/ansible-playbook-wrapper.sh" %*
exit /b %ERRORLEVEL%
