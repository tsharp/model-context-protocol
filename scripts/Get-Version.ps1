<#
.SYNOPSIS
    Calculates semantic version from git history.

.DESCRIPTION
    Version is determined by:
    1. If on a tag (v*) -> use tag version exactly
    2. If on release/v* branch -> {major}.{minor}.{commits-since-branch-or-tag}
    3. Otherwise -> {latest-tag-version}.{commits-since-tag}-dev

.EXAMPLE
    ./scripts/Get-Version.ps1
    
.EXAMPLE
    ./scripts/Get-Version.ps1 -UpdateCargo
#>

param(
    [switch]$UpdateCargo,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"

function Get-GitVersion {
    # Check if we're in a git repo
    if (-not (Test-Path ".git")) {
        $gitRoot = git rev-parse --show-toplevel 2>$null
        if (-not $gitRoot) {
            throw "Not in a git repository"
        }
        Push-Location $gitRoot
        $popLocation = $true
    }

    try {
        # Get current branch/ref
        $currentRef = git rev-parse --abbrev-ref HEAD 2>$null
        if ($currentRef -eq "HEAD") {
            # Detached HEAD - check if we're on a tag
            $currentRef = git describe --tags --exact-match 2>$null
        }

        # Check if we're exactly on a tag
        $exactTag = git describe --tags --exact-match 2>$null
        if ($exactTag -and $exactTag -match '^v?(\d+\.\d+\.\d+.*)$') {
            $version = $Matches[1]
            return @{
                Version = $version
                IsTag = $true
                IsPrerelease = $version -match '-'
                Branch = $currentRef
                CommitsSinceTag = 0
            }
        }

        # Get latest tag
        $latestTag = git describe --tags --abbrev=0 2>$null
        $commitsSinceTag = 0
        $baseVersion = "0.1.0"

        if ($latestTag -and $latestTag -match '^v?(\d+)\.(\d+)\.?(\d*)(.*)$') {
            $major = [int]$Matches[1]
            $minor = [int]$Matches[2]
            $patch = if ($Matches[3]) { [int]$Matches[3] } else { 0 }
            $suffix = $Matches[4]
            
            $baseVersion = "$major.$minor.$patch"
            $commitsSinceTag = [int](git rev-list --count "$latestTag..HEAD" 2>$null)
        }

        # Check if on a release branch
        if ($currentRef -match '^release/v?(\d+)\.(\d+)$') {
            $branchMajor = [int]$Matches[1]
            $branchMinor = [int]$Matches[2]
            
            # Count commits since branching from main (or since the branch start)
            $mergeBase = git merge-base HEAD origin/main 2>$null
            if (-not $mergeBase) {
                $mergeBase = git merge-base HEAD main 2>$null
            }
            
            if ($mergeBase) {
                $commitsSinceBranch = [int](git rev-list --count "$mergeBase..HEAD" 2>$null)
            } else {
                $commitsSinceBranch = $commitsSinceTag
            }

            # Use greater of commits since tag or commits since branch
            $patchVersion = [Math]::Max($commitsSinceTag, $commitsSinceBranch)
            
            $version = "$branchMajor.$branchMinor.$patchVersion"
            
            return @{
                Version = $version
                IsTag = $false
                IsPrerelease = $false
                Branch = $currentRef
                CommitsSinceTag = $commitsSinceTag
            }
        }

        # Default: base version + commits + dev suffix
        if ($commitsSinceTag -gt 0) {
            # Bump patch and add dev suffix
            $parts = $baseVersion -split '\.'
            $newPatch = [int]$parts[2] + $commitsSinceTag
            $version = "$($parts[0]).$($parts[1]).$newPatch-dev"
        } else {
            $version = "$baseVersion-dev"
        }

        return @{
            Version = $version
            IsTag = $false
            IsPrerelease = $true
            Branch = $currentRef
            CommitsSinceTag = $commitsSinceTag
        }
    }
    finally {
        if ($popLocation) {
            Pop-Location
        }
    }
}

function Update-CargoVersion {
    param([string]$Version)
    
    $cargoFiles = @(
        "Cargo.toml",
        "crates/mcp-macros/Cargo.toml"
    )

    foreach ($file in $cargoFiles) {
        if (Test-Path $file) {
            $content = Get-Content $file -Raw
            $content = $content -replace '(?m)^version = "[^"]*"', "version = `"$Version`""
            Set-Content $file $content -NoNewline
            if (-not $Quiet) {
                Write-Host "Updated $file to version $Version" -ForegroundColor Green
            }
        }
    }
}

# Main
$result = Get-GitVersion

if (-not $Quiet) {
    Write-Host ""
    Write-Host "Git Version Info:" -ForegroundColor Cyan
    Write-Host "  Version:      $($result.Version)" -ForegroundColor White
    Write-Host "  Branch:       $($result.Branch)" -ForegroundColor Gray
    Write-Host "  Is Tag:       $($result.IsTag)" -ForegroundColor Gray
    Write-Host "  Is Prerelease: $($result.IsPrerelease)" -ForegroundColor Gray
    Write-Host "  Commits Since Tag: $($result.CommitsSinceTag)" -ForegroundColor Gray
    Write-Host ""
}

if ($UpdateCargo) {
    Update-CargoVersion -Version $result.Version
}

# Output version for scripts
if ($Quiet) {
    Write-Output $result.Version
} else {
    return $result
}
