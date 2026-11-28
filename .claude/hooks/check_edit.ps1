# PreToolUse hook: reject edits that break AGENTS.md rules CI would catch later.
#
# Reads the tool call as JSON on stdin. Checks only the text being written
# (new_string / content), so existing code never blocks an edit. Exit 2 blocks
# the call and sends stderr back to the agent; exit 0 lets it through.
# [regex] methods are case-sensitive, like the CI greps; -match is not.

[Console]::InputEncoding = [Text.Encoding]::UTF8
$call = [Console]::In.ReadToEnd() | ConvertFrom-Json
$in = $call.tool_input

# Same patterns as the `portable` job in .github/workflows/ci.yml.
$cfgAllTest = 'cfg\(all\(test|cfg\(all\([^)]*,\s*test'
$decisionRecord = 'decision 0[0-9]{3}'
$expectAttr = '#!?\[expect\('
$allowAttr = '(?s)#!?\[allow\((.*?)\)\]'
$noVerify = '\bgit\s+commit\b[^;&|]*\s(--no-verify|-n)\b'

$problems = @()
if ($call.tool_name -eq 'Bash') {
    if ([regex]::IsMatch([string]$in.command, $noVerify)) {
        $problems += 'git commit --no-verify skips the asset-policy pre-commit hook; fix the failure instead'
    }
} else {
    $path = [string]$in.file_path
    $text = if ($null -ne $in.content) { [string]$in.content }
        elseif ($null -ne $in.new_string) { [string]$in.new_string }
        else { ($in.edits | ForEach-Object { $_.new_string }) -join "`n" }

    if ($path -match '\.(rs|md)$' -and [regex]::IsMatch($text, $decisionRecord)) {
        $problems += 'cites a decision record in the old numbered form; cite an ADR as ADR-NNNN or by path, or state the reason inline (AGENTS.md, Citations)'
    }
    if ($path -match '\.rs$') {
        if ([regex]::IsMatch($text, $cfgAllTest)) {
            $problems += 'cfg(all(test, ...)): use #[cfg(test)] and #[cfg(<platform>)] as two attributes (AGENTS.md, Pitfalls)'
        }
        if ([regex]::IsMatch($text, $expectAttr)) {
            $problems += '#[expect(...)]: CI compiles per cfg and an unfulfilled expectation fails -D warnings; use #[allow(..., reason = "...")] (AGENTS.md, Verify)'
        }
        foreach ($m in [regex]::Matches($text, $allowAttr)) {
            if (-not $m.Groups[1].Value.Contains('reason')) {
                $problems += "#[allow($($m.Groups[1].Value.Trim()))] has no reason = `"...`" saying why the code is right (AGENTS.md, hard rule 4)"
            }
            if ($m.Groups[1].Value.Contains('not_unsafe_ptr_arg_deref')) {
                $problems += 'new not_unsafe_ptr_arg_deref allow: make the fn unsafe extern "system" with a # Safety section, or take a validated type (AGENTS.md, hard rule 3)'
            }
        }
    }
}

if ($problems.Count -gt 0) {
    [Console]::Error.WriteLine('Blocked by .claude/hooks/check_edit.ps1:')
    foreach ($p in $problems) { [Console]::Error.WriteLine("- $p") }
    exit 2
}
exit 0
