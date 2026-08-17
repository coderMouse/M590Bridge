[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Wait-ForKey {
    param([Parameter(Mandatory = $true)][string]$Message)

    Write-Host $Message
    try {
        # ReadKey 不要求按 Enter，适合双击脚本时让用户看清产物路径或错误。
        $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
    }
    catch {
        # 某些重定向/非 ConsoleHost 环境不支持 RawUI；保留一个可用的回退。
        if (-not [Console]::IsInputRedirected) {
            try {
                $null = Read-Host "按 Enter 键关闭窗口"
            }
            catch {
                # 没有可读的交互输入时直接结束，不能覆盖原始打包错误。
            }
        }
    }
}

function Require-Command {
    param([Parameter(Mandatory = $true)][string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "缺少命令 '$Name'。请先安装 Windows 打包依赖，详见 ui/README.md。"
    }
}

try {
    if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
        throw "package-windows.ps1 只能在 Windows 构建机上运行。"
    }

    Require-Command "node.exe"
    Require-Command "npm.cmd"
    Require-Command "cargo.exe"
    Require-Command "rustc.exe"

    $uiDirectory = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
    $repositoryDirectory = (Resolve-Path (Join-Path $uiDirectory "..")).Path
    $packageLock = Join-Path $uiDirectory "package-lock.json"

    if (-not (Test-Path -LiteralPath $packageLock -PathType Leaf)) {
        throw "找不到 ui/package-lock.json。"
    }

    $rustVersion = & rustc.exe -vV
    if ($LASTEXITCODE -ne 0) {
        throw "无法读取 Rust toolchain 信息。"
    }

    $hostLine = $rustVersion | Where-Object { $_ -like "host:*" } | Select-Object -First 1
    if (-not $hostLine -or $hostLine -notmatch "pc-windows-msvc") {
        throw "当前 Rust 默认 host 不是 Windows MSVC；请安装并选择 stable-x86_64-pc-windows-msvc。"
    }

    Push-Location $uiDirectory
    try {
        Write-Host "正在安装锁定的 Node.js 依赖……"
        & npm.cmd ci
        if ($LASTEXITCODE -ne 0) {
            throw "npm ci 执行失败。"
        }

        Write-Host "正在构建 Windows NSIS 安装包……"
        & npm.cmd run desktop:build:windows
        if ($LASTEXITCODE -ne 0) {
            throw "Windows NSIS 构建失败。"
        }
    }
    finally {
        Pop-Location
    }

    $artifactDirectory = Join-Path $repositoryDirectory "target\release\bundle\nsis"
    $artifacts = @(Get-ChildItem -LiteralPath $artifactDirectory -Filter "*.exe" -File -ErrorAction SilentlyContinue)

    if ($artifacts.Count -eq 0) {
        throw "构建命令已结束，但 $artifactDirectory 中没有 .exe 产物。"
    }

    Write-Host ""
    Write-Host "Windows 打包完成："
    foreach ($artifact in $artifacts) {
        Write-Host "  $($artifact.FullName)"
    }
    Wait-ForKey "按任意键关闭窗口……"
    exit 0
}
catch {
    Write-Host "打包失败：$($_.Exception.Message)" -ForegroundColor Red
    Wait-ForKey "按任意键关闭窗口……"
    exit 1
}
