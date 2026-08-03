# Releasing WAID

A release is cut by pushing a version tag. The `Release` workflow
(`.github/workflows/release.yml`) triggers on any `v*` tag, builds installers
for Windows, macOS (Apple Silicon + Intel), and Linux, and attaches them to a
**draft** GitHub release.

## Steps

1. Make sure everything you want in the release is merged to `main`.

2. Tag and push:

   ```bash
   pnpm release 0.1.1    # next version — checks, tags, pushes
   ```

   The script (`scripts/release.sh`) verifies you're on a clean, up-to-date
   `main`, that the tag doesn't exist yet, and confirms before pushing. To do
   it by hand instead (check the latest tag first with
   `git tag --sort=-creatordate`):

   ```bash
   git checkout main && git pull
   git tag v0.1.1
   git push origin v0.1.1
   ```

3. Wait for the Release workflow to finish (Actions tab), then open the repo's
   **Releases** page, sanity-check the attached installers, and **publish the
   draft** — nothing goes public until you do.

## Notes

* **No in-repo version bump is needed.** The workflow stamps the version from
  the tag name into `src-tauri/tauri.conf.json` (`v0.1.1` → `0.1.1`), so the
  tag is the single source of truth for the release version.
* **Artifacts are signed.** Windows installers are signed via Azure Artifact
  Signing (GitHub OIDC — the `release` environment's `AZURE_*` secrets plus an
  Entra federated credential whose subject is exactly
  `repo:jelanijohn/waid:environment:release`); macOS builds are Developer ID
  signed and notarized (repo-level `APPLE_*` secrets). Both step groups are
  ported from [whence](https://github.com/jelanijohn/whence)'s `release.yml` —
  keep the two workflows in sync — and skip cleanly when the secrets are
  absent, so forks build unsigned.
* **A workflow re-run uses the tag's committed workflow file.** Fixes to
  `release.yml` only take effect on a *new* tag, not a re-run of an old one.
* To re-run a broken release, delete the draft release and the tag
  (`git push origin :refs/tags/v0.1.1`, `git tag -d v0.1.1`), then tag again.
