[CmdletBinding()]
param(
    [string]$Version = $(if ($env:DHTGBOT_INSTALL_VERSION) { $env:DHTGBOT_INSTALL_VERSION } else { "latest" }),
    [string]$HomeDir = $env:DHTGBOT_HOME,
    [string]$InstallDir = $env:DHTGBOT_INSTALL_DIR,
    [switch]$SkipDependencies,
    [switch]$Proxy
)

$ErrorActionPreference = "Stop"

$RepoOwner = if ($env:DHTGBOT_REMOTE_REPO_OWNER) { $env:DHTGBOT_REMOTE_REPO_OWNER } else { "haiyewei" }
$RepoName = if ($env:DHTGBOT_REMOTE_REPO_NAME) { $env:DHTGBOT_REMOTE_REPO_NAME } else { "dhtgbot" }
$RawBranch = if ($env:DHTGBOT_INSTALL_SCRIPT_BRANCH) { $env:DHTGBOT_INSTALL_SCRIPT_BRANCH } else { "master" }
$InstallScriptUrl = if ($env:DHTGBOT_INSTALL_SCRIPT_URL) {
    $env:DHTGBOT_INSTALL_SCRIPT_URL
}
else {
    "https://raw.githubusercontent.com/$RepoOwner/$RepoName/$RawBranch/scripts/install.ps1"
}

$ScriptPath = if ($PSCommandPath) { $PSCommandPath } else { $MyInvocation.MyCommand.Path }
$ScriptDir = if ($ScriptPath) { Split-Path -Parent $ScriptPath } else { $null }
$InstallScriptPath = if ($ScriptDir) { Join-Path $ScriptDir "install.ps1" } else { $null }
$TempInstallScript = $null

function Get-DefaultHomeDir {
    if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        return (Join-Path $env:LOCALAPPDATA "Programs\$RepoName\app")
    }

    return (Join-Path (Get-Location) $RepoName)
}

function Get-ExistingCommandPath {
    param(
        [string]$CommandName
    )

    $command = Get-Command $CommandName -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $command) {
        return $null
    }

    if ($command.Source -and (Test-Path $command.Source -PathType Leaf)) {
        return $command.Source
    }

    if ($command.Path -and (Test-Path $command.Path -PathType Leaf)) {
        return $command.Path
    }

    return $null
}

function Resolve-HomeFromCommandPath {
    param(
        [string]$CommandPath
    )

    if ([string]::IsNullOrWhiteSpace($CommandPath)) {
        return $null
    }

    $resolvedPath = (Resolve-Path $CommandPath -ErrorAction SilentlyContinue | Select-Object -First 1).Path
    if ([string]::IsNullOrWhiteSpace($resolvedPath)) {
        return $null
    }

    $leaf = Split-Path -Leaf $resolvedPath
    $parent = Split-Path -Parent $resolvedPath
    if ($leaf -ieq "dhtgbot.cmd") {
        foreach ($line in (Get-Content -LiteralPath $resolvedPath -ErrorAction SilentlyContinue)) {
            if ($line -match '^set "DHTGBOT_HOME=(.+)"$') {
                return $matches[1]
            }
        }

        $siblingAppRoot = Join-Path $parent "app"
        if (Test-InstalledHome -Path $siblingAppRoot) {
            return (Resolve-Path $siblingAppRoot).Path
        }

        return $parent
    }

    if (($leaf -ieq "dhtgbot.exe") -and ((Split-Path -Leaf $parent) -ieq "bin")) {
        return (Split-Path -Parent $parent)
    }

    return $null
}

function Test-InstalledHome {
    param(
        [string]$Path
    )

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $false
    }

    return (Test-Path (Join-Path $Path "config.example.yaml") -PathType Leaf) -and
        (Test-Path (Join-Path $Path "bin\dhtgbot.exe") -PathType Leaf)
}

function Resolve-ExistingHome {
    $candidates = [System.Collections.Generic.List[string]]::new()

    if (-not [string]::IsNullOrWhiteSpace($HomeDir)) {
        $candidates.Add($HomeDir) | Out-Null
    }

    if ($ScriptDir -and ((Split-Path -Leaf $ScriptDir) -ieq "scripts")) {
        $candidates.Add((Resolve-Path (Join-Path $ScriptDir "..")).Path) | Out-Null
    }

    $currentDir = (Get-Location).Path
    $candidates.Add($currentDir) | Out-Null
    $candidates.Add((Join-Path $currentDir $RepoName)) | Out-Null
    $candidates.Add((Join-Path $currentDir "$RepoName\app")) | Out-Null

    $defaultHome = Get-DefaultHomeDir
    if (-not [string]::IsNullOrWhiteSpace($defaultHome)) {
        $candidates.Add($defaultHome) | Out-Null
    }

    $commandHome = Resolve-HomeFromCommandPath -CommandPath (Get-ExistingCommandPath -CommandName "dhtgbot")
    if (-not [string]::IsNullOrWhiteSpace($commandHome)) {
        $candidates.Add($commandHome) | Out-Null
    }

    foreach ($candidate in $candidates) {
        if (Test-InstalledHome -Path $candidate) {
            return (Resolve-Path $candidate).Path
        }
    }

    throw "[dhtgbot] no existing Windows runtime installation was detected automatically. Run this script from the install root or pass -HomeDir."
}

function Invoke-DownloadWithRetry {
    param(
        [string]$Url,
        [string]$OutputPath
    )

    $maxAttempts = 5
    $parsedRetryCount = 0
    if ($env:DHTGBOT_DOWNLOAD_RETRIES -and [int]::TryParse($env:DHTGBOT_DOWNLOAD_RETRIES, [ref]$parsedRetryCount)) {
        $maxAttempts = [Math]::Max($parsedRetryCount, 1)
    }

    $parentDir = Split-Path -Parent $OutputPath
    if (-not [string]::IsNullOrWhiteSpace($parentDir)) {
        New-Item -ItemType Directory -Force -Path $parentDir | Out-Null
    }

    for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
        Remove-Item -LiteralPath $OutputPath -Force -ErrorAction SilentlyContinue

        try {
            Invoke-WebRequest -Uri $Url -OutFile $OutputPath -UseBasicParsing

            if (-not (Test-Path $OutputPath -PathType Leaf)) {
                throw "download did not produce a file"
            }

            $fileInfo = Get-Item -LiteralPath $OutputPath -ErrorAction Stop
            if ($fileInfo.Length -le 0) {
                throw "download produced an empty file"
            }

            return
        }
        catch {
            Remove-Item -LiteralPath $OutputPath -Force -ErrorAction SilentlyContinue

            if ($attempt -ge $maxAttempts) {
                throw "[dhtgbot] failed to download $Url after $attempt attempts. $($_.Exception.Message)"
            }

            Write-Warning "[dhtgbot] download failed on attempt $attempt/$maxAttempts, retrying: $($_.Exception.Message)"
            Start-Sleep -Seconds ([Math]::Min($attempt * 2, 10))
        }
    }
}

function Ensure-InstallScript {
    if ($InstallScriptPath -and (Test-Path $InstallScriptPath -PathType Leaf)) {
        return $InstallScriptPath
    }

    $script:TempInstallScript = Join-Path $env:TEMP ("dhtgbot-install-" + [System.Guid]::NewGuid().ToString("N") + ".ps1")
    Write-Host "[dhtgbot] downloading $InstallScriptUrl"
    Invoke-DownloadWithRetry -Url $InstallScriptUrl -OutputPath $script:TempInstallScript
    return $script:TempInstallScript
}

function Invoke-Upgrade {
    param(
        [string]$InstallScript
    )

    $resolvedHome = Resolve-ExistingHome
    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $InstallDir = $resolvedHome
    }

    Write-Host "[dhtgbot] upgrading runtime layout in $resolvedHome"
    if ($SkipDependencies.IsPresent) {
        Write-Host "[dhtgbot] upgrading only dhtgbot (dependency upgrades skipped)"
    }
    else {
        Write-Host "[dhtgbot] upgrading binaries: dhtgbot, amagi, tdlr, aria2"
    }

    $previousOverwrite = $env:DHTGBOT_INSTALL_OVERWRITE
    $hadOverwrite = $null -ne (Get-ChildItem Env:DHTGBOT_INSTALL_OVERWRITE -ErrorAction SilentlyContinue)
    $env:DHTGBOT_INSTALL_OVERWRITE = "always"

    try {
        $arguments = @(
            "-Source", "Remote",
            "-Version", $Version,
            "-HomeDir", $resolvedHome,
            "-InstallDir", $InstallDir
        )

        if ($SkipDependencies.IsPresent) {
            $arguments += "-SkipDependencies"
        }

        if ($Proxy.IsPresent) {
            $arguments += "-Proxy"
        }

        & powershell -ExecutionPolicy Bypass -File $InstallScript @arguments
    }
    finally {
        if ($hadOverwrite) {
            $env:DHTGBOT_INSTALL_OVERWRITE = $previousOverwrite
        }
        else {
            Remove-Item Env:DHTGBOT_INSTALL_OVERWRITE -ErrorAction SilentlyContinue
        }
    }
}

try {
    $installScript = Ensure-InstallScript
    Invoke-Upgrade -InstallScript $installScript
}
finally {
    if ($script:TempInstallScript -and (Test-Path $script:TempInstallScript -PathType Leaf)) {
        Remove-Item -LiteralPath $script:TempInstallScript -Force
    }
}
