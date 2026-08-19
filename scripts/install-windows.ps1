# ============================================================
# PairDesk Windows 一键安装（PowerShell）
#
# 用法（PowerShell 里一条命令）：
#   irm https://raw.githubusercontent.com/wmasfoe/pairdesk/main/scripts/install-windows.ps1 | iex
#
# 自动: 解析最新 release .msi → 下载 → msiexec 静默安装
# ============================================================
$ErrorActionPreference = "Stop"

$repo = "wmasfoe/pairdesk"
$app  = "PairDesk"

Write-Host "==> [1/3] 解析最新 release 的 .msi 下载地址"
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers @{ "User-Agent" = "pairdesk-installer" }
$asset = $release.assets | Where-Object { $_.name -like "*.msi" } | Select-Object -First 1
if (-not $asset) {
    Write-Host "错误: 最新 release 中没有 .msi 安装包" -ForegroundColor Red
    exit 1
}
$url = $asset.browser_download_url
Write-Host "     $url"

Write-Host "==> [2/3] 下载 msi"
$tmp = Join-Path $env:TEMP "PairDesk.install.msi"
Invoke-WebRequest $url -OutFile $tmp

Write-Host "==> [3/3] 静默安装 (msiexec)"
$p = Start-Process msiexec.exe -ArgumentList @("/i", $tmp, "/quiet", "/norestart") -Wait -PassThru
Remove-Item $tmp -Force -ErrorAction SilentlyContinue

if ($p.ExitCode -eq 0) {
    Write-Host ""
    Write-Host "===============================================" -ForegroundColor Green
    Write-Host " 已安装 $app (msiexec exit 0)" -ForegroundColor Green
    Write-Host " 从开始菜单搜索 PairDesk 启动即可" -ForegroundColor Green
    Write-Host "===============================================" -ForegroundColor Green
} else {
    Write-Host "安装失败, msiexec 退出码: $($p.ExitCode)" -ForegroundColor Red
    Write-Host "可能原因: 未以管理员运行 PowerShell (msi 安装需管理员)" -ForegroundColor Yellow
    exit 1
}
