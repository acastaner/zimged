## Agent skills

### Issue tracker

Issues are tracked in this repo's GitHub Issues, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Tests and the gate

The gate is local: `./tools/local-ci/run.sh` runs `lint` and `test` in a container on a fresh clone of the committed HEAD and records a pass as a git note on the tree (`refs/notes/gate`). It refuses a dirty tree, so commit first; fix a red gate in a new commit, not an amend. `--in-place` is for iteration and never counts. See `tools/local-ci/README.md`.
