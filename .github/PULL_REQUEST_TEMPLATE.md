## What and why

<!-- What does this change, and why? Link the issue it fixes, e.g. "Fixes #12". -->

## How it was tested

<!-- Commands you ran, platforms you tried, screenshots for UI changes. -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked` pass
- [ ] Windows-only code is behind `#[cfg(windows)]` and the other platforms still build
- [ ] User-visible changes are listed under **Unreleased** in `CHANGELOG.md`
- [ ] `docs/USER_GUIDE.md` is updated if behaviour changed
