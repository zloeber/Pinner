# Document upgrade command and ecosystems

**Trigger:** Adding or updating README / guide docs for `pinner upgrade`, mise install backends, or per-provider Pin/Upgrade/Check/Gaps pages.

## Steps

1. Keep README Install mise backends **only** as:
   ```bash
   mise use -g github:zloeber/Pinner
   mise use -g cargo:pinner
   ```
   Never document vague `mise install pinner`.
2. README Quick start must include `pinner upgrade` and `pinner upgrade --walkthrough`.
3. Provider matrix Preferred upgrade means must match `docs/superpowers/specs/2026-08-05-upgrade-subcommand-design.md` (normative table).
4. Each `docs/guide/ecosystems/<kind>.md` needs Pin / Upgrade / Check / Gaps + preferred tool. Azure Gaps: tasks are map-only (no marketplace HTTP yet).
5. Wire pages in `docs/SUMMARY.md`; update quick-start + configuration; skill: pin vs upgrade, never `--walkthrough` for agents.
6. Verify with `task docs` (mdbook).

## Gotchas

- `task docs` copies `README.md` → `docs/README.md` before build; edit root README, not a stale copy.
- Azure task upgrade hint in code explicitly says marketplace HTTP is not implemented — keep docs in sync.
