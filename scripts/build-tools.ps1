<#
.SYNOPSIS
  Builds the source-referenced diagnostic tools used by LocalToolHub on Windows.

.DESCRIPTION
  Windows counterpart of scripts/build-tools.sh. Builds the Go analyzers
  (influxql, opengemini, influxdb) and the Rust flux query analyzer to
  bin/tools/*.exe (or -OutputDir).

  The resulting .exe binaries are referenced by the `tools:` section of
  examples/logagent.yaml via each tool's `path_env` (append .exe on Windows).

.PARAMETER OutputDir
  Optional output directory. Defaults to $LOGAGENT_TOOLS_BIN_DIR, then
  $LOGAGENT_WORK_DIR/bin/tools, then <repo>/target/tools.

.PARAMETER Only
  Build only one tool: influxql | flux | opengemini | influxdb.

.EXAMPLE
  pwsh scripts/build-tools.ps1
  pwsh scripts/build-tools.ps1 -Only influxql -OutputDir bin\tools
#>

[CmdletBinding()]
param(
    [string]$OutputDir = "",
    [ValidateSet("influxql", "flux", "opengemini", "influxdb")]
    [string]$Only = ""
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

function Require-Command {
    param([string]$Name)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Write-Error "Missing required command: $Name"
        exit 1
    }
}

function Ensure-Submodule {
    param([string]$SubPath, [string]$Marker)
    $full = Join-Path $RepoRoot $SubPath
    if (-not (Test-Path (Join-Path $full $Marker))) {
        Write-Host "Initializing submodule: $SubPath"
        & git -C $RepoRoot submodule update --init --recursive $SubPath
        if ($LASTEXITCODE -ne 0) {
            Write-Error "git submodule update failed for $SubPath (run scripts/configure-tool-submodules.sh first if using custom clone URLs)"
            exit 1
        }
    }
    return $full
}

# Resolve output dir.
if (-not $OutputDir) {
    if ($env:LOGAGENT_TOOLS_BIN_DIR) {
        $OutputDir = $env:LOGAGENT_TOOLS_BIN_DIR
    } elseif ($env:LOGAGENT_WORK_DIR) {
        $OutputDir = Join-Path $env:LOGAGENT_WORK_DIR "bin\tools"
    } else {
        $OutputDir = Join-Path $RepoRoot "target\tools"
    }
}
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$buildGo = (-not $Only) -or ($Only -in @("influxql", "opengemini", "influxdb"))
$buildFlux = (-not $Only) -or ($Only -eq "flux")

if ($buildGo) {
    Require-Command "go"
}
if ($buildFlux) {
    Require-Command "cargo"
}

# Go build cache.
$GoCache = ""
if ($buildGo) {
    $GoVersion = (go env GOVERSION 2>$null)
    if (-not $GoVersion) { $GoVersion = (go version) -replace '\s+', '_' }
    $GoVersion = $GoVersion -replace '[^A-Za-z0-9_.-]', '_'
    if ($env:GOCACHE) {
        $GoCache = $env:GOCACHE
    } elseif ($env:LOGAGENT_GO_CACHE) {
        $GoCache = $env:LOGAGENT_GO_CACHE
    } else {
        $GoCache = Join-Path $env:TEMP "logagent-tools-gocache-$GoVersion"
    }
    New-Item -ItemType Directory -Force -Path $GoCache | Out-Null
    $env:GOCACHE = $GoCache
}

function Build-GoTool {
    param(
        [string]$SrcDir,
        [string]$Package,
        [string]$OutputName
    )
    $outputPath = Join-Path $OutputDir "$OutputName.exe"
    Write-Host "Building $OutputName -> $outputPath"
    Push-Location $SrcDir
    try {
        & go build -o $outputPath $Package
        if ($LASTEXITCODE -ne 0) {
            Write-Error "go build failed for $OutputName"
            exit 1
        }
    } finally {
        Pop-Location
    }
    Write-Host "Installed $OutputName -> $outputPath"
}

# --- influxql ---
if ($buildGo -and (-not $Only -or $Only -eq "influxql")) {
    $dir = Ensure-Submodule "third_party/influxql" "go.mod"
    Build-GoTool -SrcDir $dir -Package "./cmd/influxql-analyze" -OutputName "influxql-analyzer"
}

# --- flux (Rust) ---
if ($buildFlux) {
    $fluxManifest = Join-Path $RepoRoot "third_party\flux\libflux\flux-core\Cargo.toml"
    if (-not (Test-Path $fluxManifest)) {
        Ensure-Submodule "third_party/flux" "libflux/flux-core/Cargo.toml" | Out-Null
    }
    if (-not (Test-Path $fluxManifest)) {
        Write-Error "Missing Flux analyzer source: third_party/flux"
        exit 1
    }
    $outputPath = Join-Path $OutputDir "flux_query_analyzer.exe"
    Write-Host "Building flux_query_analyzer -> $outputPath"
    & cargo build --manifest-path $fluxManifest --features query-stats --release --bin query_stats
    if ($LASTEXITCODE -ne 0) {
        Write-Error "cargo build failed for flux_query_analyzer"
        exit 1
    }
    $built = Join-Path $RepoRoot "third_party\flux\libflux\target\release\query_stats.exe"
    if (-not (Test-Path $built)) {
        # Fallback: cargo target dir adjacent to the manifest.
        $built = Join-Path $RepoRoot "third_party\flux\libflux\flux-core\target\release\query_stats.exe"
    }
    Copy-Item $built $outputPath -Force
    Write-Host "Installed flux_query_analyzer -> $outputPath"
}

# --- opengemini ---
if ($buildGo -and (-not $Only -or $Only -eq "opengemini")) {
    $dir = $env:LOGAGENT_OPENGEMINI_SRC_DIR
    if (-not $dir) { $dir = Join-Path $RepoRoot "third_party\openGemini" }
    if (-not (Test-Path (Join-Path $dir "go.mod"))) {
        $dir = Ensure-Submodule "third_party/openGemini" "go.mod"
    }
    if (Test-Path (Join-Path $dir "go.mod")) {
        Build-GoTool -SrcDir $dir -Package "./app/opengemini-storage-analyzer" -OutputName "opengemini-storage-analyzer"
    } elseif ($env:LOGAGENT_OPENGEMINI_SRC_DIR -or $Only -eq "opengemini") {
        Write-Error "Missing openGemini source: $dir"
        exit 1
    } else {
        Write-Host "Skipping openGemini storage analyzer; set LOGAGENT_OPENGEMINI_SRC_DIR or initialize third_party/openGemini."
    }
}

# --- influxdb ---
if ($buildGo -and (-not $Only -or $Only -eq "influxdb")) {
    $dir = $env:LOGAGENT_INFLUXDB_SRC_DIR
    if (-not $dir) { $dir = Join-Path $RepoRoot "third_party\influxdb" }
    if (-not (Test-Path (Join-Path $dir "go.mod"))) {
        $dir = Ensure-Submodule "third_party/influxdb" "go.mod"
    }
    if (Test-Path (Join-Path $dir "go.mod")) {
        $outputPath = Join-Path $OutputDir "influxdb_storage_analyzer.exe"
        Write-Host "Building influxdb_storage_analyzer -> $outputPath"
        Push-Location $dir
        try {
            $env:GOROOT = go env GOROOT
            $env:PATH = "$($env:GOROOT)\bin;$env:PATH"
            # The upstream pkg-config.sh helper is bash-only; on Windows we rely
            # on system pkg-config being on PATH (if the build needs it) or build
            # without it.
            & go build -o $outputPath "./cmd/influxdb_storage_analyzer"
            if ($LASTEXITCODE -ne 0) {
                Write-Error "go build failed for influxdb_storage_analyzer"
                exit 1
            }
        } finally {
            Pop-Location
        }
        Write-Host "Installed influxdb_storage_analyzer -> $outputPath"
    } elseif ($env:LOGAGENT_INFLUXDB_SRC_DIR -or $Only -eq "influxdb") {
        Write-Error "Missing InfluxDB analyzer source: $dir"
        exit 1
    } else {
        Write-Host "Skipping InfluxDB storage analyzer; set LOGAGENT_INFLUXDB_SRC_DIR or initialize third_party/influxdb."
    }
}

Write-Host "build-tools.ps1 complete. Output: $OutputDir"
