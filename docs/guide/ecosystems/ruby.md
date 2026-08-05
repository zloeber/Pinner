# ruby

**Default:** on. **Preferred upgrade tool:** RubyGems HTTP (Bundler not required).

## Pin

Floating gems in `Gemfile` / related manifests. Lock → map → RubyGems HTTP (`gem` optional). Exact gem versions.

## Upgrade

RubyGems HTTP latest; optional `gem`; `PINNER_RUBY_RESOLVE_MAP`. Pin style: exact gem version.

## Check

Drift vs `pinner.lock.json`.

## Gaps

- Path / git gems are not registry-upgraded.
- Bundler lock is not the preferred upgrade evidence path.
