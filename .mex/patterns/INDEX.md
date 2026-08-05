# Pattern Index

Lookup table for all pattern files in this directory. Check here before starting any task — if a pattern exists, follow it.

<!-- This file is populated during setup (Pass 2) and updated whenever patterns are added.
     Each row maps a pattern file (or section) to its trigger — when should the agent load it?

     Format — simple (one task per file):
     | [filename.md](filename.md) | One-line description of when to use this pattern |

     Format — anchored (multi-section file, one row per task):
     | [filename.md#task-first-task](filename.md#task-first-task) | When doing the first task |
     | [filename.md#task-second-task](filename.md#task-second-task) | When doing the second task |

     Example (from a Flask API project):
     | [add-api-client.md](add-api-client.md) | Adding a new external service integration |
     | [debug-pipeline.md](debug-pipeline.md) | Diagnosing failures in the request pipeline |
     | [crud-operations.md#task-add-endpoint](crud-operations.md#task-add-endpoint) | Adding a new API route with validation |
     | [crud-operations.md#task-add-model](crud-operations.md#task-add-model) | Adding a new database model |

     Keep this table sorted alphabetically. One row per task (not per file).
     If you create a new pattern, add it here. If you delete one, remove it. -->

| Pattern | Use when |
|---------|----------|
| [core-upgrade-orchestration.md](core-upgrade-orchestration.md) | Adding or changing pinner-core `upgrade` / `upgrade_with_filter` orchestration |
| [ecosystem-upgrade-resolve.md](ecosystem-upgrade-resolve.md) | Adding `ResolveMode::Upgrade` resolve branches in an ecosystem crate |
| [fix-github-actions-node24.md](fix-github-actions-node24.md) | Node 20 Actions deprecation warnings or semantic-release PAT checkout auth failures |
| [fix-release-locked-after-version-bump.md](fix-release-locked-after-version-bump.md) | release.yml fails `cargo build --locked` after rewriting workspace version from tag |
| [fix-task-install-version-lag.md](fix-task-install-version-lag.md) | `task install` reports stale Cargo.toml version after a newer GitHub Release |
| [fix-semantic-release-chain.md](fix-semantic-release-chain.md) | Semantic-release creates a tag but release.yml never starts |
