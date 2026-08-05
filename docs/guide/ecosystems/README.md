# Ecosystems

Each provider supports **Pin** (freeze floating → exact), **Upgrade** (bump exact pins to latest), and **Check** (drift vs `pinner.lock.json`). Gaps call out what is skipped.

| Provider | Default | Preferred upgrade tool |
|----------|---------|------------------------|
| [mise](mise.md) | on | `mise latest` / `mise ls-remote` |
| [node](node.md) | on | `npm view` |
| [python](python.md) | on | **uv** |
| [docker](docker.md) | on | `docker buildx imagetools inspect` |
| [actions](actions.md) | on | `gh api` |
| [terraform](terraform.md) | on | registry HTTP |
| [helm](helm.md) | opt-in | HTTP index / OCI |
| [k8s](k8s.md) | opt-in | docker digests |
| [cargo](cargo.md) | on | crates.io HTTP |
| [go](go.md) | on | `go list -m -u` |
| [ruby](ruby.md) | on | RubyGems HTTP |
| [gitlab](gitlab.md) | opt-in | docker digests + `git ls-remote` |
| [azure](azure.md) | opt-in | docker digests; tasks via map |

Full matrix (also supported / pin style / notes): [repository README](../../README.md#provider-support). Design: [upgrade subcommand](../../superpowers/specs/2026-08-05-upgrade-subcommand-design.md).
