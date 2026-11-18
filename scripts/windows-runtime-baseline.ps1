param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 2147483647)]
    [int]$TargetPid,
    [ValidateRange(2, 300)]
    [int]$DurationSeconds = 30,
    [ValidateRange(1, 30)]
    [int]$IntervalSeconds = 5
)

$ErrorActionPreference = 'Stop'
$root = Get-CimInstance Win32_Process -Filter "ProcessId = $TargetPid" -OperationTimeoutSec 5
if ($null -eq $root -or $root.Name -ne 'netdisk-monitor.exe') {
    throw 'TargetPid must identify an already running netdisk-monitor.exe; no app was launched.'
}
$rootCreated = $root.CreationDate
$clock = [System.Diagnostics.Stopwatch]::StartNew()
$samples = [System.Collections.Generic.List[object]]::new()
$previousCpu = @{}
$previousAt = 0.0
$observedCpuSeconds = 0.0
$observedSeconds = 0.0
$previousComplete = $false
$reason = 'duration_complete'
while ($true) {
    $all = @(Get-CimInstance Win32_Process -OperationTimeoutSec 5)
    $sameRoot = $all | Where-Object { $_.ProcessId -eq $TargetPid -and $_.CreationDate -eq $rootCreated }
    if ($null -eq $sameRoot) { $reason = 'process_exited'; break }
    $tree = [System.Collections.Generic.HashSet[uint32]]::new()
    [void]$tree.Add([uint32]$TargetPid)
    $createdByPid = @{ ([uint32]$TargetPid) = $rootCreated }
    do {
        $added = $false
        foreach ($entry in $all) {
            if ($tree.Contains([uint32]$entry.ParentProcessId) -and
                $entry.CreationDate -ge $createdByPid[[uint32]$entry.ParentProcessId] -and
                $tree.Add([uint32]$entry.ProcessId)) {
                $createdByPid[[uint32]$entry.ProcessId] = $entry.CreationDate
                $added = $true
            }
        }
        if ($tree.Count -gt 2048) { throw 'Process tree exceeded the bounded 2048-process limit.' }
    } while ($added)
    $cpu = 0.0
    $private = [int64]0
    $working = [int64]0
    $unreadable = 0
    $currentCpu = @{}
    foreach ($entry in $all) {
        if (-not $tree.Contains([uint32]$entry.ProcessId)) { continue }
        $process = $null
        try {
            $process = [System.Diagnostics.Process]::GetProcessById([int]$entry.ProcessId)
            # Native StartTime guards against PID reuse between CIM and handle acquisition.
            if ([Math]::Abs(($process.StartTime.ToUniversalTime() - $entry.CreationDate.ToUniversalTime()).TotalMilliseconds) -gt 1) {
                $unreadable++
                continue
            }
            $key = "$($entry.ProcessId):$($process.StartTime.ToUniversalTime().Ticks)"
            $seconds = $process.TotalProcessorTime.TotalSeconds
            $currentCpu[$key] = $seconds
            if ($previousCpu.ContainsKey($key)) { $cpu += [Math]::Max(0, $seconds - $previousCpu[$key]) }
            $private += $process.PrivateMemorySize64
            $working += $process.WorkingSet64
        } catch { $unreadable++ }
        finally { if ($null -ne $process) { $process.Dispose() } }
    }
    $at = $clock.Elapsed.TotalSeconds
    $delta = $at - $previousAt
    $percent = $null
    $complete = $unreadable -eq 0
    if ($samples.Count -gt 0 -and $delta -gt 0 -and $complete -and $previousComplete) {
        $percent = [Math]::Round(100 * $cpu / $delta, 3)
        $observedCpuSeconds += $cpu
        $observedSeconds += $delta
    }
    $samples.Add([pscustomobject]@{
        elapsedSeconds = [Math]::Round($at, 3)
        treeProcesses = $tree.Count
        readableProcesses = $currentCpu.Count
        unreadableProcesses = $unreadable
        cpuPercentOfOneCore = $percent
        privateBytes = if ($complete) { $private } else { $null }
        workingSetBytes = if ($complete) { $working } else { $null }
    })
    $previousCpu = $currentCpu
    $previousAt = $at
    $previousComplete = $complete
    if ($at -ge $DurationSeconds) { break }
    $remaining = [Math]::Min($IntervalSeconds, $DurationSeconds - $at)
    Start-Sleep -Milliseconds ([int][Math]::Ceiling($remaining * 1000))
}
$memorySamples = @($samples | Where-Object { $null -ne $_.privateBytes })
[pscustomobject]@{
    targetPid = $TargetPid
    executable = $root.ExecutablePath
    startedUtc = $rootCreated.ToUniversalTime().ToString('o')
    stoppedReason = $reason
    observedCpuOnly = $true
    notes = 'Read-only sample. CPU excludes children that exit between polls and CPU before each identity is first sampled; working sets can share pages. This is not a 24h soak.'
    meanCpuPercentOfOneCore = if ($observedSeconds -gt 0) { 100 * $observedCpuSeconds / $observedSeconds } else { $null }
    peakPrivateBytes = if ($memorySamples.Count) { ($memorySamples | Measure-Object privateBytes -Maximum).Maximum } else { $null }
    samples = $samples.ToArray()
} | ConvertTo-Json -Depth 5
