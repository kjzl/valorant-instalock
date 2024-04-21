@echo off

REM Check if Git is installed
where /q git
if %ERRORLEVEL% neq 0 (
    echo Git is not installed. Please install Git.
    exit /b 1
)

REM Check if Rust is installed
where /q rustc
if %ERRORLEVEL% neq 0 (
    echo Rust is not installed. https://www.rust-lang.org/tools/install
	pause
	exit /b 1
)

REM Check if nightly toolchain is installed
rustup toolchain list | findstr /C:"nightly"
if %ERRORLEVEL% neq 0 (
    echo Nightly toolchain is not installed. Installing nightly...
    rustup toolchain install nightly
)

REM Set nightly toolchain as default
rustup default nightly

REM Update Rust and cargo
echo Updating Rust and cargo...
rustup update

REM Clone the git repo
echo Cloning git repo...
git clone https://github.com/kjzl/valorant-instalock.git

REM Change directory to the cloned repo
cd valorant-instalock

REM Build release binaries
echo Building binaries...
cargo build --release
cargo build

REM Move the binaries to the desired location
echo Moving binaries...
move target\release\valorant-instalock.exe ..\valorant-instalock.exe
echo .\valorant-instalock.exe
move target\debug\valorant-instalock.exe ..\valorant-instalock.debug.exe
echo .\valorant-instalock.debug.exe

REM Clean up
echo Cleaning up...
cd ..
rmdir /s /q valorant-instalock

echo Compilation completed successfully!

REM wait for user input
pause
