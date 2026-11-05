# Coding standards

Read by the reviewer agent, not the implementer. Mechanical rules live in CI (`cargo fmt`, `clippy -D warnings`, tests, `nix build`) and in tests such as `crates/rig-tui/tests/examples.rs`. What follows needs judgement.

## Docs follow behaviour

A change a user can see (a key, a screen, a config key, a file the app writes, a message) updates the doc that describes it in the same change: `docs/user-guide.md`, `config.example.toml`, and `README.md` if the summary changes. A doc still saying "planned" for something that exists, or describing something that was removed, is a finding.

## Hardware claims trace to CLAUDE.md

A statement that something works on a radio (a doc, a code comment, UI text, a capability-table row marked verified, a test name) must trace to a dated entry in `CLAUDE.md` under "Hardware status". If it does not, it says *untested on hardware*. A verbal report with no date and setup is not an entry.
