# PostToolUse hook: run rustfmt on the edited .rs file, if HEAD had it formatted.
#
# A file that was not formatted at HEAD is left alone: formatting it whole would
# turn a two-line edit into a diff of hundreds of lines. The text goes to
# rustfmt through stdin, so only this file is formatted, not the `mod` children
# it declares; rustfmt still finds rustfmt.toml and rust-toolchain.toml from its
# working directory. On stdin, `rustfmt --check` exits 0 even when it would
# change something, so the output is compared with the input instead.
# Exit 2 sends stderr back to the agent: rustfmt could not parse the file.

[Console]::InputEncoding = [Text.Encoding]::UTF8
$path = [string]([Console]::In.ReadToEnd() | ConvertFrom-Json).tool_input.file_path
if ($path -notmatch '\.rs$' -or -not (Test-Path -LiteralPath $path -PathType Leaf)) { exit 0 }

$dir = Split-Path -Parent $path
$utf8 = [Text.UTF8Encoding]::new($false)

# Runs a program in the file's directory on $stdin; output comes back unchanged.
function Invoke-Piped([string]$exe, [string[]]$arguments, [string]$stdin) {
    $psi = [Diagnostics.ProcessStartInfo]::new($exe)
    foreach ($a in $arguments) { $psi.ArgumentList.Add($a) }
    $psi.WorkingDirectory = $dir
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.StandardInputEncoding = $utf8
    $psi.StandardOutputEncoding = $utf8
    $psi.StandardErrorEncoding = $utf8
    $p = [Diagnostics.Process]::Start($psi)
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    $p.StandardInput.Write($stdin)
    $p.StandardInput.Close()
    $p.WaitForExit()
    [pscustomobject]@{ Code = $p.ExitCode; Out = $out.Result; Err = $err.Result }
}

# rustfmt run directly does not read Cargo.toml: pass the workspace edition.
function Format-Rust([string]$src) {
    Invoke-Piped 'rustfmt' @('--edition', '2024', '--emit', 'stdout') $src
}

$head = Invoke-Piped 'git' @('show', "HEAD:./$(Split-Path -Leaf $path)") ''
if ($head.Code -eq 0) {
    $headFormatted = Format-Rust $head.Out
    if ($headFormatted.Code -ne 0 -or $headFormatted.Out -cne $head.Out) { exit 0 }
}

$current = [IO.File]::ReadAllText($path, $utf8)
$formatted = Format-Rust $current
if ($formatted.Code -ne 0) {
    # rustfmt.toml sets a nightly-only option, so stable rustfmt warns on every run.
    $detail = ($formatted.Err -split "`r?`n" | Where-Object { $_ -and $_ -notmatch '^Warning: ' }) -join "`n"
    [Console]::Error.WriteLine("rustfmt could not format ${path}:`n$detail")
    exit 2
}
if ($formatted.Out -cne $current) { [IO.File]::WriteAllText($path, $formatted.Out, $utf8) }
exit 0
