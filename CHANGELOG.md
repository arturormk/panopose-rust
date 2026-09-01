# Changelog

All notable user-visible changes should be documented here.

PanoPose uses odd minor versions for development builds and even minor versions for release lines.

## v1.2

- Add an optional branded nadir cap with live radius preview and PNG/Stellarium export support.
- Keep `Save As` metadata-only: it writes unmapped, untranscoded source pixels, supports JPG/PNG/TIFF metadata paths, and stores nadir-cap settings without rendering the cap.
- Import latitude and longitude reliably from EXIF/XMP, round imported coordinates to five decimal places, accept decimal point or decimal comma input, and leave site fields blank when no geolocation metadata is present.
- Treat elevation as optional for astronomy and Stellarium export, defaulting to 0 m when blank.
- Delete EXIF GPS tags on metadata save when latitude or longitude is blank, while still saving EXIF GPS latitude/longitude when only elevation is blank.
- Disable and hide site-dependent astronomy controls when latitude/longitude are missing, warn when opening a target without site metadata, and refresh astronomy markers when the user completes the site fields.
- Make `Align Target` vertical dragging follow the expected image-drag direction.
- Fix Tauri frontend build hooks for the frontend working directory and use `dev.panopose.desktop` as the bundle identifier.
- Build `panopose-cli` in the quickstart release flow and include it alongside `panopose` in Linux `.deb` and `.rpm` packages.

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
