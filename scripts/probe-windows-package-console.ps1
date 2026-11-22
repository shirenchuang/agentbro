$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or -not $env:AGENTBRO_PACKAGE_CONSOLE_RESULT) {
    throw 'Package console probing requires an isolated GitHub-hosted runner'
}

Add-Type -Namespace AgentBro -Name PackageConsole -MemberDefinition @'
[DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
[DllImport("kernel32.dll")] public static extern uint GetConsoleCP();
[DllImport("kernel32.dll", SetLastError = true)] public static extern bool FreeConsole();
[DllImport("kernel32.dll", SetLastError = true)] public static extern bool AttachConsole(uint processId);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
'@

$toolWindow = [AgentBro.PackageConsole]::GetConsoleWindow()
$report = [ordered]@{
    toolConsoleAttached = [AgentBro.PackageConsole]::GetConsoleCP() -ne 0
    toolConsoleVisible = $toolWindow -ne [IntPtr]::Zero -and [AgentBro.PackageConsole]::IsWindowVisible($toolWindow)
}
if (-not [AgentBro.PackageConsole]::FreeConsole()) {
    throw [ComponentModel.Win32Exception]::new([Runtime.InteropServices.Marshal]::GetLastWin32Error())
}
$report.shellConsoleAttached = [AgentBro.PackageConsole]::AttachConsole([uint32]::MaxValue)
if (-not $report.shellConsoleAttached) {
    $attachError = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    if ($attachError -ne 6) { throw [ComponentModel.Win32Exception]::new($attachError) }
}
$shellWindow = [AgentBro.PackageConsole]::GetConsoleWindow()
$report.shellConsoleVisible = $shellWindow -ne [IntPtr]::Zero -and [AgentBro.PackageConsole]::IsWindowVisible($shellWindow)
[void][AgentBro.PackageConsole]::FreeConsole()
$report | ConvertTo-Json -Compress | Set-Content -LiteralPath $env:AGENTBRO_PACKAGE_CONSOLE_RESULT -Encoding utf8
if ($env:AGENTBRO_PACKAGE_FIXTURE_RESUME -and -not $env:agentbro_retry) {
    $deadline = (Get-Date).AddSeconds(45)
    while (-not (Test-Path -LiteralPath $env:AGENTBRO_PACKAGE_FIXTURE_RESUME)) {
        if ((Get-Date) -ge $deadline) { throw 'The package navigation test did not resume the first update' }
        Start-Sleep -Milliseconds 100
    }
}
