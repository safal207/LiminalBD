param(
    [string]$Store = ".\benchmark-data",
    [int]$WsPort = 8787,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

Write-Host "== LiminalDB stack demo entrypoint =="
Write-Host "Repo root  : $(Get-Location)"
Write-Host "Store path : $Store"
Write-Host "WS port    : $WsPort"

if (-not $SkipBuild) {
    Write-Host "`n[1/3] Building liminal-cli + live-benchmark (release, MSVC target)..."
    cargo +stable-x86_64-pc-windows-msvc build --release -p liminal-cli -p liminaldb-client --example live-benchmark --target x86_64-pc-windows-msvc
}

Write-Host "`n[2/3] Start LiminalDB server in another terminal:"
Write-Host "  .\target\x86_64-pc-windows-msvc\release\liminal-cli.exe --store $Store --ws-port $WsPort"

Write-Host "`n[3/3] Run baseline benchmark in another terminal:"
Write-Host "  .\target\x86_64-pc-windows-msvc\release\examples\live-benchmark.exe --url ws://127.0.0.1:$WsPort --warmup 50 --query-rounds 25 --batch-rounds 5 --batch-size 500 --timeline-top 20"

Write-Host "`nExpected signals:"
Write-Host "  - Server logs show ws_server.listening on 127.0.0.1:$WsPort"
Write-Host "  - Benchmark prints LQL p50/p95/p99 and batch ingest throughput"
Write-Host "  - Benchmark exits with code 0"

Write-Host "`nIf server exits immediately on Windows:"
Write-Host "  - Keep stdin open while running liminal-cli (example):"
Write-Host "    cmd /c ""ping -t 127.0.0.1 | .\target\x86_64-pc-windows-msvc\release\liminal-cli.exe --store $Store --ws-port $WsPort"""
