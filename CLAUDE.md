@AGENTS.md
@claude-local.md

Everything Claude Code needs to work here is in AGENTS.md, imported above.
claude-local.md adds the working rules for Claude Code sessions and their
agents (verify before claiming, how PRs are followed).

To bring the open issues back in line with the code (stale references,
issues the code shows done, a `skill:` label saying which skill works
each one, the roadmap), run `/groom-issues`.

## Agent skills

### Issue tracker

Issues live in GitHub Issues of `victordecarli/VIctory-NVIDIA`, through the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/` at the repo root, created only when needed. See `docs/agents/domain.md`.
