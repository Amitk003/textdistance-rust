# Restore the pinned reference checkout used by fuzz/ and bench/.
# reference/ is gitignored (it is the upstream original, not part of the port),
# so this script re-creates it at the exact commit the port was proven against.
# See .port-mortem.toml and tests/port/test_surface.py for the pin.

$ErrorActionPreference = "Stop"
$commit = "d6a68d61088a40eef5c88191ccf79323dbf34850"
$target = "reference\textdistance"

if (Test-Path $target) {
    Write-Output "$target already present; leaving it alone."
    exit 0
}

git clone https://github.com/life4/textdistance.git $target
git -C $target checkout $commit
Write-Output "Reference pinned at $commit."
