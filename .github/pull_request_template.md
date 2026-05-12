<!-- Thanks for the PR! Please fill out this template. Delete sections that don't apply. -->

## Summary

<!-- One sentence: what does this PR do? -->

## Motivation

<!-- Why is this change needed? Link to issue(s) if applicable. -->

Closes #

## Changes

<!-- Bullet list of the substantive changes. -->

-

## Manual testing

<!-- AppKit / IOKit / NSXPC code can't be fully tested in CI. Describe what
     you did manually to verify the change. -->

- [ ] Built locally: `./scripts/install.sh`
- [ ] Launched: `open -a OpenLid`
- [ ] Smoke-tested the affected behavior
- [ ] (if applicable) Tested with helper installed: `./scripts/dev-install-helper.sh`

## Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] `cargo fmt --check` is clean
- [ ] Documentation updated if user-visible behavior changed
- [ ] [`CHANGELOG.md`](../CHANGELOG.md) updated under `[Unreleased]` if user-visible
- [ ] Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
