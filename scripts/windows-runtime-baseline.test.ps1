$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot 'windows-runtime-baseline.ps1'
$tokens = $null
$parseErrors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile($script, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
$self = [System.Diagnostics.Process]::GetCurrentProcess()
try { $created = $self.StartTime; $self.Dispose() } catch { throw }

# Fixtures exercise serialization/coverage using this test process only; no Monitor is launched.
function Get-CimInstance {
    param($ClassName, $Filter, $OperationTimeoutSec)
    $root = [pscustomobject]@{
        ProcessId = $PID; ParentProcessId = 0; Name = 'netdisk-monitor.exe'
        CreationDate = $created; ExecutablePath = 'fixture-not-a-monitor-binary'
    }
    if ($Filter) { return $root }
    $root
    if ($includeUnreadable) {
        [pscustomobject]@{
            ProcessId = 2147483647; ParentProcessId = $PID; Name = 'unreadable-fixture.exe'
            CreationDate = $created; ExecutablePath = $null
        }
    }
}
$includeUnreadable = $false
$complete = & $script -TargetPid $PID -DurationSeconds 2 -IntervalSeconds 1 | ConvertFrom-Json
if ($complete.samples.Count -lt 2 -or $complete.stoppedReason -ne 'duration_complete') { throw 'Complete fixture failed' }
if ($null -eq $complete.meanCpuPercentOfOneCore -or $complete.peakPrivateBytes -le 0) { throw 'Complete counters missing' }
if ($complete.samples[0].cpuPercentOfOneCore -ne $null) { throw 'First CPU sample must be unavailable' }
$includeUnreadable = $true
$partial = & $script -TargetPid $PID -DurationSeconds 2 -IntervalSeconds 1 | ConvertFrom-Json
if ($null -ne $partial.meanCpuPercentOfOneCore -or $null -ne $partial.peakPrivateBytes) { throw 'Missing counters were fabricated as zero' }
foreach ($sample in $partial.samples) {
    if ($sample.unreadableProcesses -ne 1 -or $sample.privateBytes -ne $null -or $sample.cpuPercentOfOneCore -ne $null) { throw 'Partial coverage is not explicit' }
}
'Baseline parser and complete/partial read-only fixtures passed; no actual Monitor baseline measured'
