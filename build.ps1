$ErrorActionPreference = 'Stop'

Push-Location $PSScriptRoot
try {
    cargo build --locked --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed.' }

    $wasmTools = if (Test-Path './target/tools/bin/wasm-tools.exe') {
        './target/tools/bin/wasm-tools.exe'
    } else {
        'wasm-tools'
    }
    & $wasmTools component new 'target/wasm32-unknown-unknown/release/solidgear.wasm' -o 'solidgear/solidgear.wasm'
    if ($LASTEXITCODE -ne 0) { throw 'WebAssembly component build failed.' }

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem

    $packageDir = Join-Path $PSScriptRoot 'solidgear'
    $archivePath = Join-Path $PSScriptRoot 'solidgear.zip'
    $tempPath = "$archivePath.$([guid]::NewGuid()).tmp"
    try {
        $archive = [IO.Compression.ZipFile]::Open($tempPath, [IO.Compression.ZipArchiveMode]::Create)
        try {
            $entries = @('plugin.json', 'solidgear.wasm')
            $entries += Get-ChildItem (Join-Path $packageDir 'icons') -Filter '*.svg' -File |
                ForEach-Object { "icons/$($_.Name)" }
            foreach ($entry in $entries) {
                [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                    $archive, (Join-Path $packageDir $entry), $entry
                )
            }
        } finally {
            $archive.Dispose()
        }
        Move-Item -LiteralPath $tempPath -Destination $archivePath -Force
    } finally {
        if (Test-Path -LiteralPath $tempPath) {
            Remove-Item -LiteralPath $tempPath
        }
    }
} finally {
    Pop-Location
}
