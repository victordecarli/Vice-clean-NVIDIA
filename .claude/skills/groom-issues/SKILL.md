---
name: groom-issues
description: Checks every open GitHub issue against the code on main, corrects stale references, closes what the code proves done, labels each issue with the skill that should work it, and brings the roadmap up to date.
disable-model-invocation: true
argument-hint: "[issue number ...]"
---

# Groom issues

Brings the open issues back in line with the code: what each one cites
still exists where it says, its state is true, and its `skill:` label
names how to work it. With arguments, groom only those issues; the
roadmap step still runs.

Done when every issue in scope has a verdict backed by a command and its
output, the approved actions are applied, and the report is shown.

## 1. Collect

On `main` (`git fetch` first; record its short hash, every comment cites
it):

```sh
gh issue list --state open --limit 200 --json number,title,labels,updatedAt
gh issue list --state closed --limit 30 --json number,title,closedAt
gh label list --limit 100
```

Dump each issue's title, body and comments into one file in the
scratchpad (`gh issue view N --json number,title,body,comments`) and read
that file, not one call per issue.

## 2. Verify each issue against the code

For every issue, grep what it cites: the error message the code returns,
the identifiers, the `path:line` references, the switches and config
keys. Each issue gets one verdict:

| Verdict | Evidence required |
|---------|-------------------|
| **open** | the cited message or behaviour is still in the code |
| **done** | the commit that fixed it, plus the test that proves it or the message now gone |
| **superseded** | newer issues or runs show the symptom changed; name them |
| **stale refs** | still open, but its paths or lines moved (a split moves code into new modules); give the new locations |

A verdict without a command output behind it is reported as **Mentira -
Supostamente** (`claude-local.md`), never as fact. A port suggestion
("apply edsuuu `abc123`") is stale too when the file it patches has since
been split: say the port is now by hand, into which module.

## 3. Choose a skill label

Each open issue gets one `skill:` label, two when its work has two
distinct phases (find the cause, then choose a design):

| Label | Skill | The issue's state |
|-------|-------|-------------------|
| `skill: diagnóstico` | `/diagnosing-bugs` | root cause unknown; hypotheses only |
| `skill: arquitetura` | `/improve-codebase-architecture` with the direction named, then `/codebase-design` | the fix is a choice of module shape or of the seam between crates |
| `skill: tdd` | `/tdd`, or `/implement` | scope known and the RED test already describable |
| `skill: protótipo` | `/prototype` | one question only throwaway code answers (a forced setting in a local build) |
| `skill: wayfinder` | `/wayfinder`, then `/to-spec` | an epic or roadmap too foggy for one session |

Read the skill's `SKILL.md` before giving an issue a label you are unsure
of. Replace a `skill:` label that no longer fits; leave every other
label as it is.

## 4. Plan, then apply

Show one table, `issue | verdict | evidence | action | skill label`, and
apply it after the user approves. Actions:

- **open / stale refs**: a comment headed `## Status at main <hash>
  (<date>)` with the new locations and what changed. Bodies stay as their
  author wrote them; the roadmap's is the one body this skill edits.
- **superseded**: retitle to the current symptom, keeping the old one as
  `(was: ...)`, and comment with the runs that show the change.
- **done**: close with a comment naming the commit and the test.
- Write comments in English, the repository's language.

GitHub mechanics:

- Label descriptions are limited to 100 characters, counted in bytes, so
  each accent costs two.
- `gh issue edit` with a label that does not exist fails the whole edit:
  create missing labels first, then confirm each edit printed the URL.
- Re-read an issue's state right before editing it: the user may close
  issues while this skill runs.

## 5. Roadmap

The open issue titled `Roadmap: ...` holds the checklist. Rewrite its
steps section so that every open issue appears exactly once, closed ones
are checked, and the phases follow what the latest runs show blocks
progress first (the run ends > the most dropped work > translation gaps >
speed). Keep the sections before and after it unchanged.

## 6. Report

The applied table with each comment's URL, then: issues closed and why,
labels created, and every claim left as **Mentira - Supostamente**.
