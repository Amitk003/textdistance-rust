$ErrorActionPreference = "Stop"

$BasePy = "C:\Users\amitk\AppData\Local\Python\pythoncore-3.11-64\python.exe"
$VenvPy = Join-Path (Get-Location) ".venv\Scripts\python.exe"

# Run a native build tool and fail loudly on a bad exit code. maturin and cargo
# both write their build progress to stderr; under $ErrorActionPreference =
# "Stop" that stderr is treated as a PowerShell terminating error, which would
# abort the build after the extension is already built. Native stderr must
# never stop the script, only a non-zero exit code may.
function Invoke-Native {
    param([string]$File, [string[]]$Arguments)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & $File @Arguments
    $code = $LASTEXITCODE
    $ErrorActionPreference = $previous
    if ($code -ne 0) {
        throw "command failed with exit code ${code}: $File $Arguments"
    }
}

if (-not (Test-Path $VenvPy)) {
    if (-not (Test-Path $BasePy)) {
        throw "Python 3.11 not found at $BasePy. Edit build.ps1 to point at your Python install."
    }
    & $BasePy -m venv ".venv"
    if ($LASTEXITCODE -ne 0) { throw "Could not create .venv" }
}

Invoke-Native -File $VenvPy -Arguments @("-m", "pip", "install", "--quiet", "--upgrade", "pip")
Invoke-Native -File $VenvPy -Arguments @("-m", "pip", "install", "--quiet", "maturin>=1.4,<2.0", "pytest", "hypothesis")
Invoke-Native -File $VenvPy -Arguments @("-m", "maturin", "develop", "--release")
Invoke-Native -File "cargo" -Arguments @("build", "--release", "-p", "tdc")

Write-Host ""
Write-Host "Extension built and installed into .venv; CLI built at target\release\tdc.exe."
Write-Host "Run the original suite with:"
Write-Host "  .venv\Scripts\python.exe -m pytest -m 'not external' tests/original"
Write-Host "Try the CLI with:"
Write-Host "  target\release\tdc.exe distance levenshtein test text"
