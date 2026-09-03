![Made with Codex](https://img.shields.io/badge/made%20with-Codex-111111)

# PanoPose — 360° panorama alignment for astronomy/Stellarium

PanoPose is a Linux desktop application for manually orienting, registering, inspecting, and exporting full-sphere equirectangular panoramas for astronomical observing-site planning. It helps align horizon features, Alt/Az grids, astronomy markers, and night-sky references so a panorama can be calibrated for planning observations.

![PanoPose aligning a panorama against a night-sky reference](examples/github-screenshot/horizon.jpg)

The source of truth for the original product direction is [docs/BLUEPRINT.md](docs/BLUEPRINT.md).

## Current Features

- Load a target 2:1 equirectangular panorama and view it inside a zoomable Three.js sphere.
- Navigate the view independently from panorama alignment.
- Align the target panorama with yaw/azimuth, pitch/altitude, and roll/horizon-tilt controls.
- Add calibrated reference panoramas as layers.
- Compare layers using blend, target-only, reference-only, and blink modes.
- Display an Alt/Az grid with zoom-dependent spacing and cardinal labels.
- Read EXIF/XMP metadata from panoramas, including capture time, timezone offset, GPS position, elevation, and PanoPose/GPano pose metadata.
- Load time and/or site metadata from an ordinary image without loading it as a panorama layer.
- Leave site fields blank when an image has no geolocation metadata, or enter coordinates with either decimal points or decimal commas; imported latitude and longitude are rounded to five decimal places.
- Hide site-dependent astronomy controls and clear astronomy markers until latitude and longitude are present; elevation is optional and defaults to 0 m for astronomy calculations.
- Save PanoPose orientation, panorama capture time, site, GPano metadata, and nadir-cap settings back into an unmapped image using `exiftool`.
- Export the oriented target as a PNG 2:1 equirectangular panorama at the original target resolution.
- Export with South on the horizontal centerline, North at the left/right seam, and the horizon vertically centered.
- Export a Stellarium landscape ZIP containing a horizon directory with `landscape.ini` and the oriented PNG panorama.
- Keep Stellarium `angle_rotatez` at the fixed texture convention while baking the current PanoPose orientation into the exported panorama image.
- Prompt for Stellarium landscape name, author, description, ZIP directory, and texture filename without dismissing the dialog on stray outside clicks or key presses.
- Show overwrite confirmation and a row-based progress bar during long PNG and Stellarium exports.
- Preview and export an optional branded black nadir cap with adjustable angular radius.
- Detect the optional external `skyseg-ncnn` executable on `PATH` and show `Remove Sky` only when it is available.
- Toggle automatic `skyseg-ncnn` sky removal and preview the target panorama with sky transparent.
- Export PNG and Stellarium panoramas with sky removed to alpha when `Remove Sky` is enabled.
- Display Sun, Moon, planet, and bright-star markers as open circles with labels below them.
- Enable Planetarium mode to show the 1500 brightest catalog stars above the horizon for the selected site and time.
- Automatically enable Planetarium mode when EXIF-imported time places the Sun below the horizon.
- Remember the last image directory within the current app session for image-open dialogs.
- Build desktop packages with the bundled 512x512 application icon configured for Tauri release artifacts.

## Main Workflow

1. Take a picture with a 360-degree camera, or build a full-sphere 2:1 equirectangular panorama from multiple photos using stitching software.
2. Load the panorama into PanoPose. Confirm that latitude, longitude, optional elevation, and `Panorama capture time` match where and when the image was taken; blank latitude/longitude fields and hidden astronomy controls mean no geolocation metadata was found.
3. Use the `Roll (Horizontal Tilt)` spinner to tilt the panorama until the vertical Alt/Az grid lines align with vertical structures in the image.
4. Switch to `Align Target` and drag the image of the Sun onto PanoPose's Sun marker for the selected site and time.
5. Use `Save As` to write orientation/time/site metadata back into an unmapped image, `Export Pano As` to generate a calibrated PNG panorama, or `Export Stellarium ZIP` to generate a Stellarium landscape package.

For ordinary phone photos, PanoPose intentionally imports only EXIF time/site metadata. It does not attempt to project normal rectilinear photos onto the sphere, because reliable FOV estimation is usually unavailable from phone EXIF alone.

`Load EXIF from Image` updates the astronomical reference time, not the target panorama capture time. To deliberately replace the capture timestamp that `Save As` writes to EXIF, edit `Panorama capture time` or use `Use Reference Time`.

## Export Behavior

`Export Pano As` writes a derived PNG file and does not modify the source panorama.

- The export keeps the target panorama's original pixel dimensions.
- The output is a full-sphere 2:1 equirectangular image covering 360° azimuth by 180° altitude.
- South, azimuth 180°, is centered horizontally.
- The geometric horizon is centered vertically.
- The current alignment pose is baked into the output pixels so the exported image matches the preview orientation.
- The save dialog only offers PNG output; if the selected name has no `.png` suffix, PanoPose appends one.
- Existing output files require explicit overwrite confirmation.
- Long exports show a modal progress bar based on completed output rows.
- When `Nadir Cap` is enabled, export burns a black circular cap into the nadir with the PanoPose icon and name; the cap is oriented to read upright when looking South.
- When `Remove Sky` is enabled, export runs `skyseg-ncnn` on the oriented panorama and writes sky pixels as transparent alpha.

`Export Stellarium ZIP` writes a `.zip` file containing one landscape directory with `landscape.ini` and a posed PNG panorama. The exporter also fills the current latitude, longitude, and elevation, using 0 m when elevation is blank, prompts for the landscape name, author, description, ZIP directory name, and PNG filename, and keeps Stellarium's `angle_rotatez` at `-90` while baking the current PanoPose pose into the PNG. Install the resulting package using Stellarium's landscape instructions: <https://stellarium.org/landscapes.html>.

`Save As` is metadata-only. For a new output path it copies the current source image, then writes metadata; for an existing path it updates the selected image in place after confirmation. It does not remap the panorama or convert the image encoding. The save dialog accepts JPG, PNG, and TIFF paths, and PanoPose stores JSON metadata including the current nadir-cap enabled state and radius. If latitude or longitude is blank, PanoPose removes EXIF GPS tags instead of writing a partial or default location. If latitude and longitude are present but elevation is blank, EXIF GPS latitude/longitude are written without GPS altitude. The nadir cap is not rendered into `Save As` output.

## Nadir Cap

`Nadir Cap` adds a black circular overlay at altitude -90° to hide the tripod or other nadir artifacts. The spinner controls how many degrees away from the nadir the cap extends, from 1° to 45°, and the preview updates interactively.

The cap uses the PanoPose icon with a transparent background and flanking `Pano` / `Pose` label. Enabling the cap affects `Export Pano As` and `Export Stellarium ZIP`; `Save As` stores the setting for later reopening without altering pixels.

## Sky Removal

`Remove Sky` is an automatic, non-destructive mask preview for the target panorama. It depends on the external `skyseg-ncnn` executable from <https://github.com/knyipab/skyseg-ncnn>. The control is shown only when `skyseg-ncnn` is installed and available on `PATH`.

- Preview/export first renders the oriented panorama, then runs `skyseg-ncnn <corrected-pano-image> <mask.jpg>`.
- The generated mask is applied as inverted alpha: black stays opaque, white becomes transparent, and gray becomes partial transparency.
- Semi-transparent border pixels are decontaminated by subtracting the local sky color from the stored RGB.
- Toggling `Remove Sky` off and back on reuses the cached preview while the target image and alignment angles are unchanged.
- PanoPose treats sky segmentation as an optional external capability; without `skyseg-ncnn`, the rest of the app and exports continue to work normally.

## Versioning

PanoPose displays application versions with a `v` prefix.

- Odd minor versions are development lines. The built app derives the patch number from the Git commit count, so a `0.1.0` manifest build at commit count 12 displays `v0.1.12`.
- Even minor versions are release lines. Automation leaves them fixed, so `1.2.0` displays `v1.2`, and `1.2.1` is shown only after an explicit manifest bump.
- If Git metadata is unavailable on an odd-minor build, the app falls back to the manifest patch version.

## Repository Layout

- `crates/panopose-core`: reusable Rust model, coordinate math, panorama export with optional progress reporting and alpha masks, synthetic test assets, and astronomy provider boundary.
- `crates/panopose-cli`: developer CLI for generating validation panoramas and exercising core workflows.
- `src-tauri`: Tauri v2 desktop shell and Rust commands, including metadata writing, `skyseg-ncnn` integration, Stellarium ZIP creation, and blocking-thread image export with progress events.
- `frontend`: Vite + TypeScript + Three.js viewer.

## Development

### Release Quickstart

For a current-platform release build and optional install:

```sh
./quickstart-panopose.sh
```

The quickstart script:

- checks for Rust, Node.js, and npm;
- offers to install `cargo-tauri` if it is missing;
- installs frontend dependencies with `npm ci --prefix frontend`;
- can force a clean Rust and frontend rebuild with `--force-rebuild`;
- asks which final packages to build using a numbered menu, or accepts explicit `--bundles` / `--no-bundle` options;
- builds the `panopose-cli` release utility and the desktop app with `cargo tauri build --ci`;
- lists generated bundle artifacts and the release executable paths;
- offers to install the preferred generated package.

Useful options:

```sh
./quickstart-panopose.sh --skip-app-install
./quickstart-panopose.sh --no-install
./quickstart-panopose.sh --yes
./quickstart-panopose.sh --force-rebuild
./quickstart-panopose.sh --bundles deb,rpm
./quickstart-panopose.sh --no-bundle
```

On Linux, the package prompt shows a numbered menu for `deb`, `rpm`, `appimage`, `deb,rpm`, `all`, or `none`. Typed names and comma-separated combinations such as `deb,rpm` are still accepted. The default prefers `.deb` when `dpkg` is available, then `.rpm` when `rpm` is available, then `.AppImage`.

If app installation is skipped or declined, the script prints the absolute path to the release executables and any final package files that were requested and produced. Linux `.deb` and `.rpm` packages install both `panopose` and `panopose-cli` under `/usr/bin`. When installing, the installer prefers a generated `.deb` when `dpkg` is available, then `.rpm` when `rpm` is available, then `.AppImage`. AppImage installation copies the file to `~/.local/bin/panopose.AppImage` and creates a desktop entry under `~/.local/share/applications`.

The Tauri bundle config uses `src-tauri/icons/icon.png` as the release icon. The icon must remain a square RGBA PNG; the current asset is 512x512.

AppImage bundling is handled by Tauri's linuxdeploy tooling. On systems without working AppImage/FUSE support, `.deb` and `.rpm` artifacts may still be produced even when AppImage packaging fails.

### Local Development

```sh
cargo test --workspace
npm install --prefix frontend
npm run dev --prefix frontend
```

To run the desktop app after installing frontend dependencies:

```sh
cargo tauri dev
```

`Save As` metadata writing requires `exiftool` to be installed and available on `PATH`. It writes EXIF capture-time tags from `Panorama capture time`, stores the separate astronomical reference time in PanoPose XMP metadata, and keeps the source pixels unmapped and untranscoded. PNG panorama export uses the Rust `image` backend and does not require `exiftool`.

Sky removal requires `skyseg-ncnn` to be installed separately and available on `PATH`. When it is missing, PanoPose hides the `Remove Sky` toggle and exports normal opaque panoramas.

For a guided Linux build/install of `skyseg-ncnn`, run:

```sh
./quickstart-skyseg-ncnn.sh
```

The helper builds under ignored `thirdparty/ncnn` and `thirdparty/skyseg-ncnn` directories, installs to `$HOME/.local` by default, asks for permission before starting, and only offers the dated compatibility patches after an unpatched `skyseg-ncnn` build fails. Manual build notes are in [docs/HOW-TO-skyseg-ncnn.md](docs/HOW-TO-skyseg-ncnn.md).

## Data Sources

- `crates/panopose-core/data/bright_stars_1500.csv` is a generated subset of the Yale Bright Star Catalog / NASA HEASARC Bright Star Catalog, limited to the 1500 brightest valid entries by visual magnitude.
- Planetarium mode is offline after build because the star subset is bundled into `panopose-core`.
