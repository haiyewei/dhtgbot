[CmdletBinding()]
param(
    [string]$HomeDir = $env:DHTGBOT_HOME,
    [string]$InstallDir = $env:DHTGBOT_INSTALL_DIR,
    [switch]$KeepPath,
    [switch]$RemoveData,
    [switch]$KeepData
)

$ErrorActionPreference = "Stop"

if ($RemoveData -and $KeepData) {
    throw "[dhtgbot] cannot specify both -RemoveData and -KeepData."
}

$BinaryName = "dhtgbot.exe"
$LauncherName = "dhtgbot.cmd"
$ScriptPath = if ($PSCommandPath) { $PSCommandPath } else { $MyInvocation.MyCommand.Path }
$ScriptDir = if ($ScriptPath) { Split-Path -Parent $ScriptPath } else { $null }

function Get-DefaultProgramRoot {
    if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        return (Join-Path $env:LOCALAPPDATA "Programs\dhtgbot")
    }

    return (Join-Path (Get-Location) "dhtgbot")
}

function Get-DefaultHomeDir {
    $programRoot = Get-DefaultProgramRoot

    if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        return (Join-Path $programRoot "app")
    }

    return $programRoot
}

function Test-InstalledHome {
    param(
        [string]$Path
    )

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $false
    }

    return (Test-Path (Join-Path $Path "config.example.yaml") -PathType Leaf) -and
        (Test-Path (Join-Path $Path "bin\$BinaryName") -PathType Leaf)
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

function Supports-InteractivePrompt {
    try {
        return [Environment]::UserInteractive -and -not [Console]::IsInputRedirected -and -not [Console]::IsOutputRedirected
    }
    catch {
        return $false
    }
}

function Confirm-DependencyUninstall {
    param(
        [string]$DisplayName,
        [string]$Target
    )

    if (-not (Supports-InteractivePrompt)) {
        Write-Host "[dhtgbot] preserved $DisplayName at $Target (non-interactive mode)"
        return $false
    }

    $answer = Read-Host "[dhtgbot] uninstall $DisplayName at $Target? [y/N]"
    if ($answer -match '^(?i:y|yes)$') {
        return $true
    }

    Write-Host "[dhtgbot] preserved $DisplayName"
    return $false
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
    if ($leaf -ieq $LauncherName) {
        foreach ($line in (Get-Content -LiteralPath $resolvedPath -ErrorAction SilentlyContinue)) {
            if ($line -match '^set "DHTGBOT_HOME=(.+)"$') {
                return $matches[1]
            }
        }

        $parent = Split-Path -Parent $resolvedPath
        $siblingAppRoot = Join-Path $parent "app"
        if (Test-InstalledHome -Path $siblingAppRoot) {
            return (Resolve-Path $siblingAppRoot).Path
        }

        return $parent
    }

    if (($leaf -ieq $BinaryName) -and ((Split-Path -Leaf (Split-Path -Parent $resolvedPath)) -ieq "bin")) {
        return (Split-Path -Parent (Split-Path -Parent $resolvedPath))
    }

    return $null
}

function Resolve-ExistingHome {
    $candidates = [System.Collections.Generic.List[string]]::new()

    if (-not [string]::IsNullOrWhiteSpace($HomeDir)) {
        $candidates.Add($HomeDir) | Out-Null
    }

    if ($ScriptDir -and ((Split-Path -Leaf $ScriptDir) -ieq "scripts")) {
        $candidates.Add((Resolve-Path (Join-Path $ScriptDir "..") -ErrorAction SilentlyContinue).Path) | Out-Null
    }

    $candidates.Add((Get-Location).Path) | Out-Null

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

    return $null
}

function Resolve-ExistingInstallDir {
    param(
        [string]$ResolvedHome
    )

    if (-not [string]::IsNullOrWhiteSpace($InstallDir)) {
        return $InstallDir
    }

    $commandPath = Get-ExistingCommandPath -CommandName "dhtgbot"
    if (-not [string]::IsNullOrWhiteSpace($commandPath)) {
        return (Split-Path -Parent $commandPath)
    }

    if (-not [string]::IsNullOrWhiteSpace($ResolvedHome)) {
        $candidate = Join-Path $ResolvedHome $LauncherName
        if (Test-Path $candidate -PathType Leaf) {
            return $ResolvedHome
        }
    }

    return $null
}

function Test-PathEntry {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue) -or [string]::IsNullOrWhiteSpace($Entry)) {
        return $false
    }

    foreach ($segment in ($PathValue -split ";")) {
        if ($segment.TrimEnd("\") -ieq $Entry.TrimEnd("\")) {
            return $true
        }
    }

    return $false
}

function Remove-PathEntry {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue) -or [string]::IsNullOrWhiteSpace($Entry)) {
        return $PathValue
    }

    $kept = foreach ($segment in ($PathValue -split ';')) {
        if ([string]::IsNullOrWhiteSpace($segment)) {
            continue
        }

        if ($segment.TrimEnd("\") -ieq $Entry.TrimEnd("\")) {
            continue
        }

        $segment
    }

    return ($kept -join ';')
}

function Remove-DirectoryIfEmpty {
    param(
        [string]$Path
    )

    if (-not (Test-Path $Path -PathType Container)) {
        return
    }

    $children = Get-ChildItem -LiteralPath $Path -Force
    if ($children.Count -gt 0) {
        return
    }

    Remove-Item -LiteralPath $Path -Force
    Write-Host "[dhtgbot] removed empty directory $Path"
}

function Resolve-DependencyBinaryPath {
    param(
        [string]$InstallDirValue,
        [string]$BinaryFileName,
        [string]$CommandName
    )

    if (-not [string]::IsNullOrWhiteSpace($InstallDirValue)) {
        $candidate = Join-Path $InstallDirValue $BinaryFileName
        if (Test-Path $candidate -PathType Leaf) {
            return $candidate
        }
    }

    $commandPath = Get-ExistingCommandPath -CommandName $CommandName
    if (-not [string]::IsNullOrWhiteSpace($commandPath)) {
        return $commandPath
    }

    $fallback = Join-Path $HOME ".local\bin\$BinaryFileName"
    if (Test-Path $fallback -PathType Leaf) {
        return $fallback
    }

    return $null
}

function Uninstall-DependencyBinary {
    param(
        [string]$DisplayName,
        [string]$BinaryPath
    )

    if ([string]::IsNullOrWhiteSpace($BinaryPath)) {
        return $false
    }

    if (-not (Confirm-DependencyUninstall -DisplayName $DisplayName -Target $BinaryPath)) {
        return $false
    }

    Remove-Item -LiteralPath $BinaryPath -Force
    Write-Host "[dhtgbot] removed $BinaryPath"
    Remove-DirectoryIfEmpty -Path (Split-Path -Parent $BinaryPath)
    return $true
}

function Maybe-UninstallAmagi {
    $installDirValue = if ($env:AMAGI_INSTALL_DIR) { $env:AMAGI_INSTALL_DIR } elseif ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "Programs\amagi\bin" } else { $null }
    $binaryPath = Resolve-DependencyBinaryPath -InstallDirValue $installDirValue -BinaryFileName "amagi.exe" -CommandName "amagi"
    return (Uninstall-DependencyBinary -DisplayName "amagi" -BinaryPath $binaryPath)
}

function Maybe-UninstallTdlr {
    $installDirValue = if ($env:TDLR_INSTALL_DIR) { $env:TDLR_INSTALL_DIR } elseif ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "Programs\tdlr\bin" } else { $null }
    $binaryPath = Resolve-DependencyBinaryPath -InstallDirValue $installDirValue -BinaryFileName "tdlr.exe" -CommandName "tdlr"
    return (Uninstall-DependencyBinary -DisplayName "tdlr" -BinaryPath $binaryPath)
}

function Maybe-UninstallAria2 {
    $installDirValue = if ($env:ARIA2_INSTALL_DIR) { $env:ARIA2_INSTALL_DIR } elseif ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "Programs\aria2\bin" } else { $null }
    $binaryPath = Resolve-DependencyBinaryPath -InstallDirValue $installDirValue -BinaryFileName "aria2c.exe" -CommandName "aria2c"
    return (Uninstall-DependencyBinary -DisplayName "aria2" -BinaryPath $binaryPath)
}

$resolvedHome = Resolve-ExistingHome
if ([string]::IsNullOrWhiteSpace($resolvedHome)) {
    Write-Host "[dhtgbot] no existing runtime installation was detected automatically."
    Write-Host "[dhtgbot] workspace checkouts are not removed automatically; delete that directory manually if needed."
    exit 0
}

$resolvedInstallDir = Resolve-ExistingInstallDir -ResolvedHome $resolvedHome

$removedAny = $false
$launcherPath = if ([string]::IsNullOrWhiteSpace($resolvedInstallDir)) { $null } else { Join-Path $resolvedInstallDir $LauncherName }
if ($launcherPath -and (Test-Path $launcherPath -PathType Leaf)) {
    Remove-Item -LiteralPath $launcherPath -Force
    Write-Host "[dhtgbot] removed $launcherPath"
    $removedAny = $true
}

foreach ($path in @(
        (Join-Path $resolvedHome "bin"),
        (Join-Path $resolvedHome "scripts")
    )) {
    if (Test-Path $path) {
        Remove-Item -LiteralPath $path -Recurse -Force
        Write-Host "[dhtgbot] removed $path"
        $removedAny = $true
    }
}

$templatePath = Join-Path $resolvedHome "config.example.yaml"
if (Test-Path $templatePath -PathType Leaf) {
    Remove-Item -LiteralPath $templatePath -Force
    Write-Host "[dhtgbot] removed $templatePath"
    $removedAny = $true
}

if ($RemoveData) {
    if (Test-Path $resolvedHome) {
        Remove-Item -LiteralPath $resolvedHome -Recurse -Force
        Write-Host "[dhtgbot] removed $resolvedHome"
        $removedAny = $true
    }
}
else {
    Write-Host "[dhtgbot] preserved runtime data in $resolvedHome"
    Write-Host "[dhtgbot] kept config.yaml, data, logs, and any other user files"
    Remove-DirectoryIfEmpty -Path $resolvedHome
}

if (-not $KeepPath -and -not [string]::IsNullOrWhiteSpace($resolvedInstallDir)) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (Test-PathEntry -PathValue $userPath -Entry $resolvedInstallDir) {
        [Environment]::SetEnvironmentVariable("Path", (Remove-PathEntry -PathValue $userPath -Entry $resolvedInstallDir), "User")
        Write-Host "[dhtgbot] removed matching user PATH entry"
    }

    if (Test-PathEntry -PathValue $env:Path -Entry $resolvedInstallDir) {
        $env:Path = Remove-PathEntry -PathValue $env:Path -Entry $resolvedInstallDir
        Write-Host "[dhtgbot] removed matching current-session PATH entry"
    }
}

if (-not [string]::IsNullOrWhiteSpace($resolvedInstallDir)) {
    Remove-DirectoryIfEmpty -Path $resolvedInstallDir
}

if (Maybe-UninstallAmagi) {
    $removedAny = $true
}

if (Maybe-UninstallTdlr) {
    $removedAny = $true
}

if (Maybe-UninstallAria2) {
    $removedAny = $true
}

if ($removedAny) {
    Write-Host "[dhtgbot] uninstall complete"
}
else {
    Write-Host "[dhtgbot] nothing was removed"
}
