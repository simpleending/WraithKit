param([Parameter(Mandatory)][string]$BinDir)
# Offline CLI + HTTP smoke test. Uses only disposable fixture data and loopback.
$ErrorActionPreference = 'Stop'
$BinDir = (Resolve-Path -LiteralPath $BinDir).Path
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$work = Join-Path $tempRoot ('wraithkit-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
New-Item -ItemType Directory -Path (Join-Path $work 'tmp') | Out-Null

function Assert($Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Wait-State([string]$Base, [string]$Status) {
    for ($n = 0; $n -lt 100; $n++) {
        $state = Invoke-RestMethod -Uri "$Base/api/state" -TimeoutSec 5
        if ($state.status -eq $Status) { return $state }
        if ($state.status -in @('failed', 'needs_review')) { throw "Unexpected state: $($state.status)" }
        Start-Sleep -Milliseconds 50
    }
    throw "Timed out waiting for $Status"
}

function Wait-Pending([string]$Base, [string]$Id) {
    for ($n = 0; $n -lt 100; $n++) {
        $state = Invoke-RestMethod -Uri "$Base/api/state" -TimeoutSec 5
        if ($state.status -eq 'waiting' -and @($state.proposals | Where-Object { $_.id -eq $Id -and $_.status -eq 'pending' }).Count -eq 1) { return $state }
        if ($state.status -in @('failed', 'needs_review', 'finished')) { throw "Missing pending $Id in $($state.status)" }
        Start-Sleep -Milliseconds 50
    }
    throw "Timed out waiting for proposal $Id"
}

function Check-Local([string]$Exe, [string]$Target, [int]$ExpectedExit) {
    $start = [Diagnostics.ProcessStartInfo]::new($Exe)
    $start.ArgumentList.Add('check')
    $start.ArgumentList.Add($Target)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($start)
    try {
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        Assert ($process.WaitForExit(10000)) 'Local checker timed out'
        Assert ($process.ExitCode -eq $ExpectedExit) "Unexpected local checker exit $($process.ExitCode): $($stderr.GetAwaiter().GetResult())"
        return $stdout.GetAwaiter().GetResult()
    } finally {
        if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
        $process.Dispose()
    }
}

try {
    foreach ($app in @('security-wraith', 'release-wraith', 'repo-wraith', 'research-wraith')) {
      $scenarios = if ($app -in @('security-wraith', 'release-wraith')) { @('approve', 'deny', 'stale') } else { @('approve') }
      foreach ($scenario in $scenarios) {
        $ext = if ($IsWindows) { '.exe' } else { '' }
        $exe = Join-Path $BinDir "$app$ext"
        $start = [Diagnostics.ProcessStartInfo]::new($exe)
        $start.ArgumentList.Add('demo')
        $start.WorkingDirectory = $work
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        foreach ($name in @('TEMP', 'TMP', 'TMPDIR')) { $start.Environment[$name] = Join-Path $work 'tmp' }
        $start.Environment.Remove('WRAITH_API_KEY') | Out-Null
        $process = [Diagnostics.Process]::Start($start)
        $stderr = $process.StandardError.ReadToEndAsync()
        try {
            $base = $null
            $run = $null
            $fixture = $false
            for ($n = 0; $n -lt 10 -and -not $base; $n++) {
                $next = $process.StandardOutput.ReadLineAsync()
                Assert ($next.Wait(10000)) "$app did not print its local URL"
                $line = $next.Result
                if ($null -eq $line) { throw "$app exited before serving: $($stderr.GetAwaiter().GetResult())" }
                if ($line -eq 'FIXTURE: scripted, not a live model') { $fixture = $true }
                if ($line -match '^run folder: (.+)$') { $run = Join-Path $work $Matches[1] }
                if ($line -match '^open (http://127\.0\.0\.1:\d+/[a-f0-9]+/)$') { $base = $Matches[1].TrimEnd('/') }
            }
            Assert ($fixture -and $base -and $run) "$app must identify fixture mode, run folder and loopback URL"
            $page = Invoke-WebRequest -Uri "$base/" -TimeoutSec 5
            Assert ($page.StatusCode -eq 200) "$app UI failed"
            $state = Wait-State $base 'waiting'
            Assert ($state.model.StartsWith('FIXTURE:')) "$app API must label fixture mode"
            $expected = if ($app -eq 'repo-wraith') { 2 } else { 1 }
            Assert (@($state.proposals).Count -eq $expected) "$app proposal count"
            if ($app -eq 'release-wraith') {
                $beforeCheck = Check-Local $exe $state.target 1 | ConvertFrom-Json
                Assert ($beforeCheck.status -eq 'FAIL' -and -not $beforeCheck.ci_rerun -and -not $beforeCheck.published) 'Release checker must distinguish local FAIL from hosted CI'
            }
            if ($app -eq 'security-wraith') {
                $beforeCheck = Check-Local $exe $state.target 0
                Assert ($beforeCheck.Contains('no live 1-SEC connector or enforcement')) 'Security checker needs the integration limitation'
            }
            if ($scenario -ne 'approve') {
                $proposal = $state.proposals | Where-Object id -eq 'P1'
                if ($scenario -eq 'stale') {
                    $name = if ($app -eq 'security-wraith') { 'incident.json' } else { 'ci.log' }
                    [IO.File]::AppendAllText((Join-Path $state.target $name), "`n", [Text.UTF8Encoding]::new($false))
                }
                $body = @{ proposal = 'P1'; approve = ($scenario -eq 'stale'); digest = $proposal.digest } | ConvertTo-Json
                $answer = Invoke-RestMethod -Method Post -Uri "$base/api/decide" -ContentType 'application/json' -Body $body -TimeoutSec 5
                if ($scenario -eq 'deny') { Assert ($answer.outcome -eq 'denied') "$app denial failed" }
                else { Assert ($answer.outcome.StartsWith('stale:')) "$app changed input did not reject approval" }
                $state = Wait-State $base 'finished'
                if ($app -eq 'security-wraith') {
                    Assert (-not (Test-Path -LiteralPath (Join-Path $state.target 'response-plan.json'))) 'Denied/stale security proposal wrote a plan'
                } else {
                    $plan = Get-Content -LiteralPath (Join-Path $state.target 'release-plan.json') -Raw | ConvertFrom-Json
                    Assert ($plan.artifact -eq 'dist/spectral-cli.zip') 'Denied/stale release proposal changed the plan'
                    $afterCheck = Check-Local $exe $state.target 1 | ConvertFrom-Json
                    Assert ($afterCheck.status -eq 'FAIL') 'Denied/stale release check incorrectly passed'
                }
            } else {
                # Each app uses the same digest-bound decision API. Repo's poisoned P2 is denied.
                foreach ($proposal in ($state.proposals | Sort-Object id -Descending)) {
                    $approve = -not ($app -eq 'repo-wraith' -and $proposal.id -eq 'P2')
                    $body = @{ proposal = $proposal.id; approve = $approve; digest = $proposal.digest } | ConvertTo-Json
                    $answer = Invoke-RestMethod -Method Post -Uri "$base/api/decide" -ContentType 'application/json' -Body $body -TimeoutSec 5
                    $outcome = if ($approve) { 'approved' } else { 'denied' }
                    Assert ($answer.outcome -eq $outcome) "$app returned $($answer.outcome) instead of $outcome"
                }
                if ($app -eq 'security-wraith') {
                    $state = Wait-Pending $base 'P2'
                    $plan = Get-Content -LiteralPath (Join-Path $state.target 'response-plan.json') -Raw | ConvertFrom-Json
                    Assert ($plan.simulation_only -and $plan.simulated_state -eq 'paused' -and $plan.revision -eq 1) 'First security approval did not write only the paused local simulation'
                    $proposal = $state.proposals | Where-Object id -eq 'P2'
                    $body = @{ proposal = 'P2'; approve = $true; digest = $proposal.digest } | ConvertTo-Json
                    $answer = Invoke-RestMethod -Method Post -Uri "$base/api/decide" -ContentType 'application/json' -Body $body -TimeoutSec 5
                    Assert ($answer.outcome -eq 'approved') 'Security rollback needs its own approved decision'
                }
                $state = Wait-State $base 'finished'
                switch ($app) {
                    'repo-wraith' {
                        $money = Get-Content -LiteralPath (Join-Path $state.target 'invoice/money.py') -Raw
                        $ci = Get-Content -LiteralPath (Join-Path $state.target '.github/workflows/ci.yml') -Raw
                        Assert ($money.Contains('round(amount * 100)')) 'Approved Repo edit missing'
                        Assert (-not $ci.Contains('curl')) 'Denied CI edit was written'
                    }
                    'research-wraith' {
                        Assert (Test-Path -LiteralPath (Join-Path $state.target 'briefs/hollow-hacks-deadline.md')) 'Approved research brief missing'
                        Assert (@($state.claims | Where-Object status -eq 'conflicted').Count -gt 0) 'Source conflict missing'
                        Assert (@($state.claims | Where-Object status -eq 'unsupported').Count -gt 0) 'Unsupported quote missing'
                    }
                    'security-wraith' {
                        $plan = Get-Content -LiteralPath (Join-Path $state.target 'response-plan.json') -Raw | ConvertFrom-Json
                        Assert ($plan.simulation_only -and $plan.simulated_state -eq 'running' -and $plan.revision -eq 2) 'Second security approval did not reverse only the local simulation'
                        Assert (@($state.claims | Where-Object status -eq 'conflicted').Count -gt 0) 'Security conflicting context was omitted'
                    }
                    'release-wraith' {
                        $plan = Get-Content -LiteralPath (Join-Path $state.target 'release-plan.json') -Raw | ConvertFrom-Json
                        Assert ($plan.artifact -eq 'artifacts/spectral-cli-1.4.2.zip') 'Approved release path repair missing'
                        $afterCheck = Check-Local $exe $state.target 0 | ConvertFrom-Json
                        Assert ($afterCheck.status -eq 'PASS' -and $afterCheck.artifact_present_and_sha256_verified) 'Actual local artifact verification did not pass'
                        Assert ($afterCheck.historical_ci_conclusion -eq 'failure' -and -not $afterCheck.ci_rerun -and -not $afterCheck.published) 'Local repair falsely claims hosted CI success or publication'
                    }
                }
            }
            $verified = Invoke-RestMethod -Method Post -Uri "$base/api/verify" -ContentType 'application/json' -Body '{}' -TimeoutSec 5
            Assert ($verified.all_ok -and @($verified.checks).Count -eq 7) "$app HTTP verification failed"
            $cli = & $exe verify $run
            Assert ($LASTEXITCODE -eq 0) "$app CLI verification failed: $cli"
            Write-Output "${app} ${scenario}: fixture, HTTP decision, actual local output/check and seven ledger checks passed"
        } finally {
            if (-not $process.HasExited) { $process.Kill($true) }
            $process.WaitForExit()
            $process.Dispose()
        }
      }
    }
} finally {
    $resolved = [IO.Path]::GetFullPath($work)
    Assert ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($resolved).StartsWith('wraithkit-smoke-')) 'Unsafe cleanup path'
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
