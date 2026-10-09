# Releasing SetMTU

Releases are built and published by GitHub Actions. Pushing a version tag is all it takes.

## Day to day

Commit and push as usual. Every push to `main` and every pull request runs the tests
([CI workflow](.github/workflows/ci.yml)). Check results on the
[Actions page](https://github.com/riaanjutte/SetMTU/actions) or with:

```bash
gh run list --limit 3
```

A red ✗ means the tests failed. Open the run to see why.

## Releasing a new version

1. **Test the app.** Run it, then Apply and Restore an MTU change on a real adapter.
2. **Bump the version** in `Cargo.toml`, for example `version = "0.1.1"`.
3. **Commit and push:**

   ```bash
   git commit -am "Release v0.1.1"
   git push
   ```

4. **Tag the release and push the tag.** This starts the [Release workflow](.github/workflows/release.yml):

   ```bash
   git tag v0.1.1
   git push origin v0.1.1
   ```

5. **Wait about 4 minutes.** The workflow runs the tests, builds the zip with `package.ps1` and
   publishes a GitHub release with the zip, `SHA256SUMS.txt` and release notes. Follow it with:

   ```bash
   gh run watch
   ```

6. **Scan the release on VirusTotal.** On the [URL tab](https://www.virustotal.com/gui/home/url), submit
   the zip's download link, for example
   `https://github.com/riaanjutte/SetMTU/releases/download/v0.1.1/SetMTU-v0.1.1-win-x64.zip`.
   Then add the zip and exe report links to the release notes (**Edit** on the release page). The
   reports are at `https://www.virustotal.com/gui/file/<sha256>`, using the hashes from `SHA256SUMS.txt`.
7. **Check the winget PR** the workflow opened (see [winget](#winget) below).
8. **Announce it** (for example on Discord) with the release link, the exe's SHA-256 and the
   VirusTotal link. `package.ps1` prints a ready-made post when run locally.

The tag must match the version in `Cargo.toml` (`v0.1.1` for `0.1.1`). If it doesn't, the workflow
stops before publishing anything.

## winget

SetMTU is published to the Windows Package Manager as `riaanjutte.SetMTU`, so users can run
`winget install riaanjutte.SetMTU` and get updates with `winget upgrade`.

**After each release**, the Release workflow's `winget` job opens a pull request to
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) with the new version. Microsoft's
bots validate and scan it, and it's usually merged within a day or two. Watch for review comments
on that PR.

**One-time setup** for the automatic PRs:

1. Fork [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) to your account (no need to clone it).
2. Create a classic personal access token with the `public_repo` scope
   (GitHub → Settings → Developer settings → Personal access tokens → Tokens (classic)).
3. Add it to this repo as an Actions secret named `WINGET_TOKEN`
   (repo → Settings → Secrets and variables → Actions).

Without the secret, the job is skipped with a notice and nothing else is affected.

The manifests for the first submission (v0.1.0) are in [`winget/`](winget/), using the same folder
layout as winget-pkgs. Validate changes with `winget validate --manifest <folder>`.

## Test builds without releasing

- **On GitHub:** Actions tab → **Release** → **Run workflow**. The zip is attached to the run under
  **Artifacts**. Nothing is published.
- **Locally:**

  ```bash
  powershell -ExecutionPolicy Bypass -File .\package.ps1
  ```

  The zip and checksums go to `dist\`. This takes a few minutes because release builds use
  link-time optimisation; use plain `cargo build` while developing.

## If a release fails

| Problem | Fix |
|---|---|
| Tag doesn't match the `Cargo.toml` version | Delete the tag (below), fix `Cargo.toml`, commit, tag again |
| Tests or build fail | Fix the code, delete the tag (below), tag again |
| Publishing fails after a successful build | Download the zip from the run's **Artifacts** and publish by hand: `gh release create v0.1.1 SetMTU-v0.1.1-win-x64.zip SHA256SUMS.txt --title "SetMTU v0.1.1"` |

Deleting a tag locally and on GitHub:

```bash
git tag -d v0.1.1
git push origin :v0.1.1
```

## What gets built

- `SetMTU-v<version>-win-x64.zip` containing `SetMTU.exe`, `README.txt`, `LICENSE.txt` and
  `Inter-OFL.txt`.
- `SHA256SUMS.txt` with checksums for the zip and the exe.
- The exe has the C runtime linked in (no Visual C++ Redistributable needed) and carries version
  details from `Cargo.toml` and `build.rs`. The Rust version is pinned in `rust-toolchain.toml`.
