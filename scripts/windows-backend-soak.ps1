param([ValidateRange(10,604800)][int]$Seconds = 86400)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$previous = $env:NETDISK_SOAK_SECONDS
try {
    $env:NETDISK_SOAK_SECONDS = [string]$Seconds
    cargo test --offline --manifest-path "$root/src-tauri/Cargo.toml" --features desktop --lib collector::tests::backend_soak -- --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Backend soak failed ($LASTEXITCODE)" }
} finally {
    $env:NETDISK_SOAK_SECONDS = $previous
}
