$ErrorActionPreference = "Stop"

$BasePy = "C:\Users\amitk\AppData\Local\Python\pythoncore-3.11-64\python.exe"
$VenvPy = Join-Path (Get-Location) ".venv\Scripts\python.exe"

if (-not (Test-Path $VenvPy)) {
    if (-not (Test-Path $BasePy)) {
        throw "Python 3.11 not found at $BasePy. Edit build.ps1 to point at your Python install."
    }
    & $BasePy -m venv ".venv"
    if (-not $?) { throw "Could not create .venv" }
}

& $VenvPy -m pip install --quiet --upgrade pip
& $VenvPy -m pip install --quiet "maturin>=1.4,<2.0" pytest hypothesis
& $VenvPy -m maturin develop --release

Write-Host ""
Write-Host "Extension built and installed into .venv."
Write-Host "Run the original suite with:"
Write-Host "  .venv\Scripts\python.exe -m pytest -m 'not external' tests/original"
