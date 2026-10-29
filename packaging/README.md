# `packaging/`: release artifacts and file associations

Spec §8.5 and §9. A pushed tag `vX.Y.Z` runs `.github/workflows/release.yml`, which builds these
and publishes a GitHub release. GitHub runs no tests: its first job, `gate-record`, refuses a tree
that the local gate (`tools/local-ci/`) never passed.

| Path | What it is |
|---|---|
| `linux/zimged.desktop` | The desktop entry. `MimeType=` lists every format zimged reads (§5.1) |
| `linux/zimged.svg` | The icon (hicolor, scalable) |
| `linux/io.github.acastaner.zimged.metainfo.xml` | AppStream metadata. Needs a `<release>` for each version |
| `linux/build.sh` | Builds the tarball, `.deb`, `.rpm`, AUR files and Flatpak staging from a built binary |
| `aur/PKGBUILD.in`, `aur/.SRCINFO.in` | AUR templates (a source build of the tag); `build.sh pkgbuild` fills in version and hash |
| `flatpak/io.github.acastaner.zimged.yml` | Flatpak manifest; repackages the tarball's binary |
| `release_notes.py` | Release notes: `release-notes/vX.Y.Z.md` plus the `tools/bench` numbers |
| `release-notes/` | One `vX.Y.Z.md` and its `vX.Y.Z.bench-*.json` per release |

The `.deb` and `.rpm` metadata (dependencies, assets) is in `crates/zimged/Cargo.toml`.

## Default handler: never

zimged registers as **a** handler for its formats and never makes itself the default (§8.5). No
package ships a `mimeapps.list`, runs `xdg-mime default`, or has a maintainer script that does.
Debian and RPM triggers update the desktop database, which only lists zimged under *Open with*.

## Releasing

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`) and run `cargo check` so
   `Cargo.lock` follows.
2. Add `<release version="X.Y.Z" date="YYYY-MM-DD"/>` at the top of `<releases>` in the metainfo.
3. On the reference machine (i5-12600KF, balanced power profile, Wayland), build the release binary
   and run the benchmarks:

   ```bash
   cargo build --release --locked -p zimged
   tools/bench/bench.py fixture tools/bench/images
   tools/bench/bench.py cold tools/bench/images/IMG_0001.jpg --out packaging/release-notes/vX.Y.Z.bench-cold.json
   ```

4. Write `packaging/release-notes/vX.Y.Z.md` and check the result with
   `packaging/release_notes.py X.Y.Z`.
5. Commit all of it, then run the gate: `./tools/local-ci/run.sh`. It pushes `refs/notes/gate`.
6. Merge to `master`, tag the gated commit `vX.Y.Z` and push the tag.
7. After the release: publish the AUR files (`zimged-X.Y.Z-aur.tar.gz`) to the AUR by hand.

## Building locally

```bash
cargo build --release --locked -p zimged
cargo install --locked cargo-deb@3.8.0 cargo-generate-rpm@0.21.1   # once
packaging/linux/build.sh dist tarball deb rpm
git archive --prefix=zimged-X.Y.Z/ -o /some/dir/source.tar.gz HEAD
packaging/linux/build.sh dist pkgbuild /some/dir/source.tar.gz flatpak
flatpak-builder --user --force-clean build-dir dist/flatpak/io.github.acastaner.zimged.yml
```

The release builds in `rust:<channel>-slim-bookworm`, the gate's base image, so its binary needs
glibc 2.36 or later.
