[CmdletBinding()]
param(
    [ValidateSet("Auto", "Local", "Remote")]
    [string]$Source = $(if ($env:DHTGBOT_INSTALL_SOURCE) { $env:DHTGBOT_INSTALL_SOURCE } else { "Auto" }),
    [string]$InstallDir = $env:DHTGBOT_INSTALL_DIR,
    [string]$HomeDir = $env:DHTGBOT_HOME,
    [string]$Version = $(if ($env:DHTGBOT_INSTALL_VERSION) { $env:DHTGBOT_INSTALL_VERSION } else { "latest" }),
    [switch]$SkipDependencies,
    [switch]$Proxy
)

$ErrorActionPreference = "Stop"

$BinaryName = "dhtgbot.exe"
$LauncherName = "dhtgbot.cmd"
$RemoteRepoOwner = if ($env:DHTGBOT_REMOTE_REPO_OWNER) { $env:DHTGBOT_REMOTE_REPO_OWNER } else { "haiyewei" }
$RemoteRepoName = if ($env:DHTGBOT_REMOTE_REPO_NAME) { $env:DHTGBOT_REMOTE_REPO_NAME } else { "dhtgbot" }
$RemoteBaseUrl = $env:DHTGBOT_REMOTE_BASE_URL
$ProxyPrefix = if ($Proxy) { "https://mirror.ghproxy.com/" } else { "" }

$AmagiVersion = if ($env:AMAGI_INSTALL_VERSION) { $env:AMAGI_INSTALL_VERSION } else { "latest" }
$AmagiRepoOwner = if ($env:AMAGI_REMOTE_REPO_OWNER) { $env:AMAGI_REMOTE_REPO_OWNER } else { "bandange" }
$AmagiRepoName = if ($env:AMAGI_REMOTE_REPO_NAME) { $env:AMAGI_REMOTE_REPO_NAME } else { "amagi-rs" }
$AmagiBaseUrl = $env:AMAGI_REMOTE_BASE_URL
$AmagiInstallDir = if ($env:AMAGI_INSTALL_DIR) { $env:AMAGI_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\amagi\bin" }

$TdlrVersion = if ($env:TDLR_INSTALL_VERSION) { $env:TDLR_INSTALL_VERSION } else { "latest" }
$TdlrRepoOwner = if ($env:TDLR_REMOTE_REPO_OWNER) { $env:TDLR_REMOTE_REPO_OWNER } else { "haiyewei" }
$TdlrRepoName = if ($env:TDLR_REMOTE_REPO_NAME) { $env:TDLR_REMOTE_REPO_NAME } else { "tdlr" }
$TdlrBaseUrl = $env:TDLR_REMOTE_BASE_URL
$TdlrInstallDir = if ($env:TDLR_INSTALL_DIR) { $env:TDLR_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\tdlr\bin" }

$Aria2Version = if ($env:ARIA2_INSTALL_VERSION) { $env:ARIA2_INSTALL_VERSION } else { "1.37.0" }
$Aria2Tag = if ($env:ARIA2_INSTALL_TAG) { $env:ARIA2_INSTALL_TAG } else { "release-$Aria2Version" }
$Aria2BaseUrl = $env:ARIA2_REMOTE_BASE_URL
$Aria2InstallDir = if ($env:ARIA2_INSTALL_DIR) { $env:ARIA2_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\aria2\bin" }
$OverwritePolicy = if ($env:DHTGBOT_INSTALL_OVERWRITE) { $env:DHTGBOT_INSTALL_OVERWRITE } else { "prompt" }

$ScriptPath = if ($PSCommandPath) { $PSCommandPath } else { $MyInvocation.MyCommand.Path }
$ScriptDir = if ($ScriptPath) { Split-Path -Parent $ScriptPath } else { $null }
$RepoRoot = if ($ScriptDir) {
    try {
        (Resolve-Path (Join-Path $ScriptDir "..") -ErrorAction Stop).Path
    }
    catch {
        $null
    }
}
else {
    $null
}
$script:RemoteTempPaths = [System.Collections.Generic.List[string]]::new()
$script:LastInstallAction = $null

function Get-DefaultInstallDir {
    return (Join-Path $env:LOCALAPPDATA "Programs\dhtgbot\bin")
}

function Get-DefaultHomeDir {
    return (Join-Path $env:LOCALAPPDATA "Programs\dhtgbot\app")
}

function Test-RepositoryWorkspace {
    return $RepoRoot -and (Test-Path (Join-Path $RepoRoot "Cargo.toml") -PathType Leaf)
}

function Get-LocalSourceBinary {
    $candidates = @()

    if ($RepoRoot) {
        $candidates += (Join-Path $RepoRoot $BinaryName)
        $candidates += (Join-Path $RepoRoot "target\release\$BinaryName")
        $candidates += (Join-Path $RepoRoot "target\debug\$BinaryName")
    }

    if ($ScriptDir) {
        $candidates += (Join-Path $ScriptDir $BinaryName)
    }

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate -PathType Leaf) {
            return (Resolve-Path $candidate).Path
        }
    }

    return $null
}

function Get-LocalTemplatePath {
    $candidates = @()

    if ($RepoRoot) {
        $candidates += (Join-Path $RepoRoot "config.example.yaml")
    }

    if ($ScriptDir) {
        $candidates += (Join-Path $ScriptDir "config.example.yaml")
    }

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate -PathType Leaf) {
            return (Resolve-Path $candidate).Path
        }
    }

    return $null
}

function Get-LocalScriptsDir {
    if ($RepoRoot) {
        $candidate = Join-Path $RepoRoot "scripts"
        if (Test-Path $candidate -PathType Container) {
            return (Resolve-Path $candidate).Path
        }
    }

    return $null
}

function Build-LocalReleaseBinary {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        return $null
    }

    if (-not (Test-RepositoryWorkspace)) {
        return $null
    }

    Write-Host "[dhtgbot] no local binary found, building release binary with cargo build --release --bin dhtgbot"

    Push-Location $RepoRoot
    try {
        & $cargo.Source build --release --bin dhtgbot
    }
    finally {
        Pop-Location
    }

    $builtBinary = Join-Path $RepoRoot "target\release\$BinaryName"
    if (Test-Path $builtBinary -PathType Leaf) {
        return (Resolve-Path $builtBinary).Path
    }

    return $null
}

function Get-DhtgbotRemoteAssetName {
    switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        ([System.Runtime.InteropServices.Architecture]::X64) { return "dhtgbot-x86_64-pc-windows-msvc.zip" }
        default { throw "[dhtgbot] unsupported system architecture for remote install: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)" }
    }
}

function Get-AmagiRemoteAssetName {
    switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        ([System.Runtime.InteropServices.Architecture]::X64) { return "amagi-x86_64-pc-windows-msvc.zip" }
        default { throw "[dhtgbot] unsupported architecture for amagi install: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)" }
    }
}

function Get-TdlrRemoteAssetName {
    switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        ([System.Runtime.InteropServices.Architecture]::X64) { return "tdlr-x86_64-pc-windows-msvc.zip" }
        default { throw "[dhtgbot] unsupported architecture for tdlr install: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)" }
    }
}

function Get-Aria2RemoteAssetName {
    switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        ([System.Runtime.InteropServices.Architecture]::X64) { return "aria2-$Aria2Version-win-64bit-build1.zip" }
        ([System.Runtime.InteropServices.Architecture]::X86) { return "aria2-$Aria2Version-win-32bit-build1.zip" }
        default { throw "[dhtgbot] unsupported architecture for aria2 install: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)" }
    }
}

function Get-RemoteDownloadUrl {
    param(
        [string]$AssetName,
        [string]$RepoOwner,
        [string]$RepoName,
        [string]$PackageVersion,
        [string]$BaseUrl
    )

    if (-not [string]::IsNullOrWhiteSpace($BaseUrl)) {
        return ($BaseUrl.TrimEnd("/") + "/$AssetName")
    }

    if ($PackageVersion -eq "latest") {
        return "${ProxyPrefix}https://github.com/$RepoOwner/$RepoName/releases/latest/download/$AssetName"
    }

    return "${ProxyPrefix}https://github.com/$RepoOwner/$RepoName/releases/download/$PackageVersion/$AssetName"
}

function Get-Aria2DownloadUrl {
    $assetName = Get-Aria2RemoteAssetName

    if (-not [string]::IsNullOrWhiteSpace($Aria2BaseUrl)) {
        return ($Aria2BaseUrl.TrimEnd("/") + "/$assetName")
    }

    return "${ProxyPrefix}https://github.com/aria2/aria2/releases/download/$Aria2Tag/$assetName"
}

function Expand-RemotePackage {
    param(
        [string]$Url,
        [string]$AssetName
    )

    $tempRoot = Join-Path $env:TEMP ("dhtgbot-install-" + [System.Guid]::NewGuid().ToString("N"))
    $archivePath = Join-Path $tempRoot $AssetName
    $extractDir = Join-Path $tempRoot "extract"

    New-Item -ItemType Directory -Force -Path $extractDir | Out-Null
    Write-Host "[dhtgbot] downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $archivePath -UseBasicParsing
    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDir -Force

    $script:RemoteTempPaths.Add($tempRoot) | Out-Null
    return $extractDir
}

function Test-PathEntry {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    foreach ($segment in ($PathValue -split ";")) {
        if ($segment.TrimEnd("\") -ieq $Entry.TrimEnd("\")) {
            return $true
        }
    }

    return $false
}

function Test-PathEntryIsFirst {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    foreach ($segment in ($PathValue -split ";")) {
        if ([string]::IsNullOrWhiteSpace($segment)) {
            continue
        }

        return $segment.TrimEnd("\") -ieq $Entry.TrimEnd("\")
    }

    return $false
}

function Set-PathEntryFirst {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($Entry)) {
        return $PathValue
    }

    $updatedSegments = [System.Collections.Generic.List[string]]::new()
    $updatedSegments.Add($Entry)

    if (-not [string]::IsNullOrWhiteSpace($PathValue)) {
        foreach ($segment in ($PathValue -split ";")) {
            if ([string]::IsNullOrWhiteSpace($segment)) {
                continue
            }

            if ($segment.TrimEnd("\") -ieq $Entry.TrimEnd("\")) {
                continue
            }

            $updatedSegments.Add($segment)
        }
    }

    return ($updatedSegments -join ";")
}

function Add-InstallDirToUserPath {
    param(
        [string]$Entry
    )

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $hadUserEntry = Test-PathEntry -PathValue $userPath -Entry $Entry
    $newUserPath = Set-PathEntryFirst -PathValue $userPath -Entry $Entry

    if ($userPath -ne $newUserPath) {
        [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")

        if ($hadUserEntry) {
            Write-Host "[dhtgbot] moved install directory to the front of the user PATH"
        }
        else {
            Write-Host "[dhtgbot] added install directory to the front of the user PATH"
        }
    }
    else {
        Write-Host "[dhtgbot] install directory already has priority in the user PATH"
    }

    $processPath = $env:Path
    $hadProcessEntry = Test-PathEntry -PathValue $processPath -Entry $Entry
    $newProcessPath = Set-PathEntryFirst -PathValue $processPath -Entry $Entry

    if ($processPath -ne $newProcessPath) {
        $env:Path = $newProcessPath

        if ($hadProcessEntry) {
            Write-Host "[dhtgbot] moved install directory to the front of PATH for the current PowerShell session"
        }
        else {
            Write-Host "[dhtgbot] updated PATH for the current PowerShell session"
        }
    }
    elseif (Test-PathEntryIsFirst -PathValue $processPath -Entry $Entry) {
        Write-Host "[dhtgbot] install directory already has priority in the current PowerShell session"
    }
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

function Confirm-Overwrite {
    param(
        [string]$DisplayName,
        [string]$ExistingPath,
        [string]$TargetPath
    )

    switch ($OverwritePolicy.ToLowerInvariant()) {
        "always" {
            Write-Host "[dhtgbot] overwrite policy is always; replacing existing $DisplayName"
            return $true
        }
        "never" {
            Write-Host "[dhtgbot] overwrite policy is never; keeping existing $DisplayName at $ExistingPath"
            return $false
        }
        "prompt" {
            if (-not [Environment]::UserInteractive) {
                Write-Warning "[dhtgbot] existing $DisplayName detected at $ExistingPath; non-interactive mode defaults to skip overwrite"
                return $false
            }

            Write-Host "[dhtgbot] existing $DisplayName detected"
            Write-Host "  current: $ExistingPath"
            Write-Host "  target : $TargetPath"
            $answer = Read-Host "Overwrite existing $DisplayName? [y/N]"
            return $answer -match '^(?i:y|yes)$'
        }
        default {
            throw "[dhtgbot] unsupported overwrite policy: $OverwritePolicy"
        }
    }
}

function Resolve-ExecutionMode {
    switch ($Source.ToLowerInvariant()) {
        "local" { return "local" }
        "remote" { return "remote" }
        "auto" {
            $hasScriptBinary = $ScriptDir -and (Test-Path (Join-Path $ScriptDir $BinaryName) -PathType Leaf)
            $hasRepoBinary = $RepoRoot -and (Test-Path (Join-Path $RepoRoot $BinaryName) -PathType Leaf)
            $hasRepoRoot = Test-RepositoryWorkspace
            $hasBuiltBinary = $RepoRoot -and (Test-Path (Join-Path $RepoRoot "target\release\$BinaryName") -PathType Leaf)

            if ($hasScriptBinary -or $hasRepoBinary -or $hasRepoRoot -or $hasBuiltBinary) {
                return "local"
            }

            return "remote"
        }
        default {
            throw "[dhtgbot] unsupported install source mode: $Source"
        }
    }
}

function Install-RemoteBinary {
    param(
        [string]$AssetName,
        [string]$RepoOwner,
        [string]$RepoName,
        [string]$PackageVersion,
        [string]$BinaryNameToInstall,
        [string]$CommandName,
        [string]$DisplayName,
        [string]$BinaryInstallDir,
        [string]$BaseUrl
    )

    $url = Get-RemoteDownloadUrl -AssetName $AssetName -RepoOwner $RepoOwner -RepoName $RepoName -PackageVersion $PackageVersion -BaseUrl $BaseUrl
    $extractDir = Expand-RemotePackage -Url $url -AssetName $AssetName
    $binaryPath = Get-ChildItem -Path $extractDir -Filter $BinaryNameToInstall -Recurse -File | Select-Object -First 1

    if (-not $binaryPath) {
        throw "[dhtgbot] binary $BinaryNameToInstall was not found in the downloaded package."
    }

    New-Item -ItemType Directory -Force -Path $BinaryInstallDir | Out-Null
    $targetPath = Join-Path $BinaryInstallDir $BinaryNameToInstall
    $existingPath = Get-ExistingCommandPath -CommandName $CommandName
    if (-not $existingPath -and (Test-Path $targetPath -PathType Leaf)) {
        $existingPath = $targetPath
    }

    if ($existingPath) {
        if (-not (Confirm-Overwrite -DisplayName $DisplayName -ExistingPath $existingPath -TargetPath $targetPath)) {
            Write-Host "[dhtgbot] kept existing $DisplayName"
            $script:LastInstallAction = "skipped"
            return
        }
    }

    Copy-Item -LiteralPath $binaryPath.FullName -Destination (Join-Path $BinaryInstallDir $BinaryNameToInstall) -Force
    Add-InstallDirToUserPath -Entry $BinaryInstallDir
    $script:LastInstallAction = "installed"
}

function Install-Amagi {
    $assetName = Get-AmagiRemoteAssetName
    Install-RemoteBinary `
        -AssetName $assetName `
        -RepoOwner $AmagiRepoOwner `
        -RepoName $AmagiRepoName `
        -PackageVersion $AmagiVersion `
        -BinaryNameToInstall "amagi.exe" `
        -CommandName "amagi" `
        -DisplayName "amagi" `
        -BinaryInstallDir $AmagiInstallDir `
        -BaseUrl $AmagiBaseUrl

    if ($script:LastInstallAction -eq "installed") {
        Write-Host "[dhtgbot] amagi installed to $(Join-Path $AmagiInstallDir 'amagi.exe')"
    }
}

function Install-Tdlr {
    $assetName = Get-TdlrRemoteAssetName
    Install-RemoteBinary `
        -AssetName $assetName `
        -RepoOwner $TdlrRepoOwner `
        -RepoName $TdlrRepoName `
        -PackageVersion $TdlrVersion `
        -BinaryNameToInstall "tdlr.exe" `
        -CommandName "tdlr" `
        -DisplayName "tdlr" `
        -BinaryInstallDir $TdlrInstallDir `
        -BaseUrl $TdlrBaseUrl

    if ($script:LastInstallAction -eq "installed") {
        Write-Host "[dhtgbot] tdlr installed to $(Join-Path $TdlrInstallDir 'tdlr.exe')"
    }
}

function Install-Aria2 {
    $assetName = Get-Aria2RemoteAssetName
    $url = Get-Aria2DownloadUrl
    $extractDir = Expand-RemotePackage -Url $url -AssetName $assetName
    $binaryPath = Get-ChildItem -Path $extractDir -Filter "aria2c.exe" -Recurse -File | Select-Object -First 1

    if (-not $binaryPath) {
        throw "[dhtgbot] binary aria2c.exe was not found in the downloaded package."
    }

    New-Item -ItemType Directory -Force -Path $Aria2InstallDir | Out-Null
    $targetPath = Join-Path $Aria2InstallDir "aria2c.exe"
    $existingPath = Get-ExistingCommandPath -CommandName "aria2c"
    if (-not $existingPath -and (Test-Path $targetPath -PathType Leaf)) {
        $existingPath = $targetPath
    }

    if ($existingPath) {
        if (-not (Confirm-Overwrite -DisplayName "aria2" -ExistingPath $existingPath -TargetPath $targetPath)) {
            Write-Host "[dhtgbot] kept existing aria2"
            $script:LastInstallAction = "skipped"
            return
        }
    }

    Copy-Item -LiteralPath $binaryPath.FullName -Destination (Join-Path $Aria2InstallDir "aria2c.exe") -Force
    Add-InstallDirToUserPath -Entry $Aria2InstallDir
    $script:LastInstallAction = "installed"
    Write-Host "[dhtgbot] aria2 installed to $(Join-Path $Aria2InstallDir 'aria2c.exe')"
}

function Write-Launcher {
    param(
        [string]$LauncherPath,
        [string]$AppRuntimeRoot
    )

    $runtimeBinary = Join-Path $AppRuntimeRoot "bin\dhtgbot.exe"
    $content = @"
@echo off
setlocal
set "DHTGBOT_HOME=$AppRuntimeRoot"
pushd "%DHTGBOT_HOME%"
"$runtimeBinary" %*
set "EXITCODE=%ERRORLEVEL%"
popd
exit /b %EXITCODE%
"@

    $encoding = New-Object System.Text.UTF8Encoding -ArgumentList $false
    [System.IO.File]::WriteAllText($LauncherPath, $content, $encoding)
}

function Install-SupportScripts {
    param(
        [string]$SourceScriptsDir
    )

    if ([string]::IsNullOrWhiteSpace($SourceScriptsDir) -or -not (Test-Path $SourceScriptsDir -PathType Container)) {
        return
    }

    $targetScriptsDir = Join-Path $HomeDir "scripts"
    Remove-Item -LiteralPath $targetScriptsDir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $targetScriptsDir | Out-Null
    Copy-Item -LiteralPath (Join-Path $SourceScriptsDir "*") -Destination $targetScriptsDir -Recurse -Force
}

function Install-DhtgbotRuntime {
    param(
        [string]$SourceBinary,
        [string]$SourceTemplate,
        [string]$SourceScriptsDir
    )

    $runtimeBinDir = Join-Path $HomeDir "bin"
    $runtimeBinary = Join-Path $runtimeBinDir $BinaryName
    $templatePath = Join-Path $HomeDir "config.example.yaml"
    $configPath = Join-Path $HomeDir "config.yaml"
    $launcherPath = Join-Path $InstallDir $LauncherName

    New-Item -ItemType Directory -Force -Path $runtimeBinDir | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $HomeDir "data") | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $HomeDir "logs") | Out-Null
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    Copy-Item -LiteralPath $SourceBinary -Destination $runtimeBinary -Force

    if ($SourceTemplate -and (Test-Path $SourceTemplate -PathType Leaf)) {
        Copy-Item -LiteralPath $SourceTemplate -Destination $templatePath -Force
        if (Test-Path $configPath -PathType Leaf) {
            Write-Host "[dhtgbot] kept existing config at $configPath"
        }
        else {
            Write-Host "[dhtgbot] config file is not created automatically; copy $templatePath to $configPath"
        }
    }

    Install-SupportScripts -SourceScriptsDir $SourceScriptsDir
    Write-Launcher -LauncherPath $launcherPath -AppRuntimeRoot $HomeDir
    Add-InstallDirToUserPath -Entry $InstallDir
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $InstallDir = Get-DefaultInstallDir
}

if ([string]::IsNullOrWhiteSpace($HomeDir)) {
    $HomeDir = Get-DefaultHomeDir
}

if (-not $SkipDependencies) {
    Install-Amagi
    Install-Tdlr
    Install-Aria2
}

$InstallMode = Resolve-ExecutionMode
$SourceBinary = $null
$SourceTemplate = $null
$SourceScriptsDir = $null

try {
    if ($InstallMode -eq "local") {
        $SourceBinary = Get-LocalSourceBinary
        $SourceTemplate = Get-LocalTemplatePath
        $SourceScriptsDir = Get-LocalScriptsDir

        if (-not $SourceBinary) {
            $SourceBinary = Build-LocalReleaseBinary
        }

        if (-not $SourceBinary) {
            throw "[dhtgbot] no local binary found in the extracted package or target\release."
        }
    }
    else {
        $assetName = Get-DhtgbotRemoteAssetName
        $url = Get-RemoteDownloadUrl -AssetName $assetName -RepoOwner $RemoteRepoOwner -RepoName $RemoteRepoName -PackageVersion $Version -BaseUrl $RemoteBaseUrl
        $extractDir = Expand-RemotePackage -Url $url -AssetName $assetName
        $binary = Get-ChildItem -Path $extractDir -Filter $BinaryName -Recurse -File | Select-Object -First 1

        if (-not $binary) {
            throw "[dhtgbot] binary $BinaryName was not found in the downloaded package."
        }

        $SourceBinary = $binary.FullName
        $template = Get-ChildItem -Path $extractDir -Filter "config.example.yaml" -Recurse -File | Select-Object -First 1
        if ($template) {
            $SourceTemplate = $template.FullName
        }

        $scriptsDir = Get-ChildItem -Path $extractDir -Directory -Filter "scripts" -Recurse | Select-Object -First 1
        if ($scriptsDir) {
            $SourceScriptsDir = $scriptsDir.FullName
        }
    }

    Install-DhtgbotRuntime -SourceBinary $SourceBinary -SourceTemplate $SourceTemplate -SourceScriptsDir $SourceScriptsDir
    Write-Host "[dhtgbot] installed launcher to $(Join-Path $InstallDir $LauncherName)"
    Write-Host "[dhtgbot] application home: $HomeDir"
    Write-Host "[dhtgbot] copy the example config before the first real run:"
    Write-Host "  Copy-Item $(Join-Path $HomeDir 'config.example.yaml') $(Join-Path $HomeDir 'config.yaml')"
    Write-Host "[dhtgbot] then edit $(Join-Path $HomeDir 'config.yaml')"
    Write-Host "[dhtgbot] confirm services.amagi.start_command and services.tdlr.start_command in config.yaml"
    Write-Host "[dhtgbot] if you use X polling, fill bots.xdl.twitter.cookies in config.yaml"
    Write-Host "[dhtgbot] the installed commands are now available in PATH: dhtgbot, amagi, tdlr, aria2c"
}
finally {
    foreach ($path in $script:RemoteTempPaths) {
        if (Test-Path $path -PathType Leaf) {
            Remove-Item -LiteralPath $path -Force
        }
        elseif (Test-Path $path -PathType Container) {
            Remove-Item -LiteralPath $path -Recurse -Force
        }
    }
}
