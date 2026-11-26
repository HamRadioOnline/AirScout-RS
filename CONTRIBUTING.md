# Contributing to AirScout-RS

Thank you for helping. This file says how changes reach `main`.

## Branches

- Never commit to `main` directly. Every change starts on its own feature branch.
- Branch from the current `main`. Name the branch after what it does, for example
  `fix/rigctld-timeout` or `feat/cat-tab`.
- Keep one topic per branch. Unrelated changes go on separate branches.

## Linear history (required)

`main` has a linear history. Merge commits are not accepted.

- Bring your branch up to date with `git rebase`, not `git merge`:

  ```sh
  git fetch origin
  git rebase origin/main
  git push --force-with-lease
  ```

- Do not merge `main` into your branch. A pull request that contains a merge commit is sent back
  for a rebase.
- Pull requests land by squash or rebase, never by a merge commit.
- Set `git config pull.rebase true` so that `git pull` does not create merge commits.

## Commits

- Write small commits with a clear subject, in the form `type(scope): summary`, for example
  `fix(as-cat): retry Doppler leave until online`. Common types: `feat`, `fix`, `test`, `docs`.
- Clean up fixup commits before review (`git rebase -i origin/main` on your own machine).

## Before you open a pull request

Run the same gate as CI, and make sure it passes:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

- Tests must not need the network beyond `127.0.0.1`.
- Do not put personal data in code, tests, fixtures or commit messages: no real call signs,
  email addresses, station coordinates or keys. Use synthetic values.
- Changes to prediction behaviour need a test and an entry in `docs/logic-findings.md`. This
  covers propagation, elevation, the crossing calculation and the aircraft positions.

## License

By contributing you agree that your contribution is licensed under GPL-3.0-only, like the rest
of the project. See [LICENSE](LICENSE).
