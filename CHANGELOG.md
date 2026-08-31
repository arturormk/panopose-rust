# Changelog

All notable user-visible changes should be documented here.

PanoPose uses odd minor versions for development builds and even minor versions for release lines.

## v1.0.0

First stable release candidate for public Linux desktop distribution.

- Publish Linux desktop installers through GitHub Releases.
- Build `.deb`, `.rpm`, and `.AppImage` packages with Tauri.
- Keep the standalone CLI available from source builds, but out of release assets.

## v0.1

Initial public development release.

- Load, view, align, compare, and export full-sphere equirectangular panoramas.
- Save PanoPose and GPano orientation metadata with `exiftool`.
- Display Sun, Moon, planet, and bright-star markers for a selected site and time.
- Preview and export optional automatic sky removal as transparency.
- Package Linux desktop builds with Tauri.
