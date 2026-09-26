# Releasing

Releases are built by `.github/workflows/release.yml` ([cargo-dist](https://opensource.axo.dev/cargo-dist/))
when a version tag is pushed: prebuilt `knot` and `knot-lsp` binaries for macOS
(arm64, x86_64), Linux (x86_64) and Windows (x86_64), their installers, and the
VS Code extension (`.vsix`). A tag with a suffix (`v0.4.0-rc.1`) is published
as a GitHub **pre-release**: GitHub's "latest" release and the Homebrew
publication skip it.

Tags are immutable: never move or delete a pushed tag. A mistake in a release
candidate is fixed by the next candidate (`-rc.2`).

## 1. Prepare the release pull request

On a branch `release/vX.Y.Z`:

1. **Versions**, all in lockstep (the tag must match the crates' version):
   - `crates/knot-core`, `crates/knot-cli`, `crates/knot-lsp`: `version` in
     `Cargo.toml`, then `cargo update --workspace --offline` for `Cargo.lock`;
   - `editors/vscode`: `npm version X.Y.Z --no-git-tag-version` (updates
     `package.json` and `package-lock.json`);
   - `knot-typst-package/typst.toml`.

   Use `X.Y.Z-rc.N` for a release candidate. The citation (`CITATION.cff`: `version`
   and `date-released`; the BibTeX entry in `README.md`) changes only for the
   final version.
2. **`CHANGELOG.md`**: the section of the version, with compatibility notes and
   known limitations.
3. **Checks**, the CI jobs run locally (see [Contributing](./dev-contributing.md)), plus the ignored R/Python
   tests (`cargo test -p knot-core -- --ignored`,
   `cargo test -p knot-cli --test integration_pdf -- --ignored`,
   `cargo test -p knot-lsp -- --ignored`).
4. **Showcase**, if the rendering changed: `scripts/render-showcase.sh` (the
   Anscombe PDF and excerpts) and `demo/record.py` (the README demo; see
   `demo/README.md`). Review them by eye.

## 2. Publish a release candidate

After the pull request is merged, from an up-to-date `master`:

```bash
git tag v0.4.0-rc.1
```

```bash
git push origin v0.4.0-rc.1
```

Check that the release workflow succeeds and that the pre-release lists the
binaries, the installers (`knot-cli-installer.sh`, `knot-cli-installer.ps1`,
`knot-lsp-…`) and `knot-X.Y.Z-rc.N.vsix`.

## 3. Validate

First run **Actions → Install smoke test → Run workflow** with the tag
(`.github/workflows/install-smoke.yml`): on Linux, macOS and Windows runners
where Knot has never been installed, it installs the release with the
documented commands, checks the versions and the user `PATH`, runs the README
quick start and builds Anscombe (the reference with `--strict --no-snapshots`,
the errors variant, which must fail).

Then, on a clean machine for each advertised platform, using only the published
instructions:

- install the candidate: `KNOT_VERSION=vX.Y.Z-rc.N` with `install.sh` (macOS,
  Linux), or the two PowerShell installers from `releases/download/vX.Y.Z-rc.N/`
  (Windows), then the `.vsix`;
- follow the README quick start, then build `examples/anscombe` with
  `knot build --strict --no-snapshots` (it must succeed) and its errors variant
  (it must fail, with the errors shown in the PDF);
- in VS Code: open the preview, edit a calculation, introduce and fix an error.

Ask an external tester to do the same, and record the outcome in the release
issue.

## 4. Publish the final version

A pull request sets the final version `X.Y.Z` (step 1, citation included) and
dates the changelog section. After it is merged, tag `vX.Y.Z` and push it as in
step 2. Check the release: it becomes GitHub's "latest", so `install.sh` and the
documented Windows commands install it.
