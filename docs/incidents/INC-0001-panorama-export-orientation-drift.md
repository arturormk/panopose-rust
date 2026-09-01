# INC-0001-panorama-export-orientation-drift
Status: Closed
Date: 2026-08-31

## Context / Trigger
The sky removal migration to `skyseg-ncnn` required exporting a pose-corrected panorama, running segmentation on that corrected image, and remapping the resulting mask back onto the source panorama. The preview looked correct, but exported PNG and Stellarium ZIP textures did not match it.

## Symptom
The exported panorama went through several incorrect states:

- Same orientation as the unposed source image.
- Reversed alpha polarity in the exported image while preview alpha was correct.
- Large azimuth offsets, including centers near 294 degrees, 300 degrees, and later 20 degrees instead of 180 degrees.
- Horizon altitude drift of a few degrees.
- A horizontally mirrored exported panorama.

## Root Cause
The implementation mixed three different coordinate contracts:

- The preview displays the panorama on a Three.js `SphereGeometry` scaled with `scale(-1, 1, 1)` and rotated with Euler order `YXZ`.
- The core exporter resamples output world directions back into source directions through `Orientation::world_alt_az_to_source`.
- Saved PNG/Stellarium textures need to be normal exported rasters, not raw viewer texture space.

Matching the quaternion alone was insufficient. Rust first used a different yaw/pitch/roll composition than Three.js, then later used the wrong horizontal source texture convention. Attempts to fix this with base yaw or source mapping changes moved the error between pose offset and horizontal mirroring because preview mapping and export raster presentation were not treated as separate concerns.

## Resolution
The final implementation separates the responsibilities:

- Rust export pose construction now matches the preview's Three.js `Euler(pitch, -90 + yaw, roll, "YXZ")` quaternion formula exactly.
- Core resampling uses a dedicated `viewer_texture_*` mapping for interpreting source pixels the same way the preview sphere does.
- Export image and Stellarium ZIP paths apply a single final raster finalization step so the saved PNG presentation is not horizontally mirrored.
- Skyseg mask remapping uses the same source texture mapping as export, keeping alpha aligned with the posed image.
- Stellarium `angle_rotatez` remains fixed at `-90` as a separate INI convention.

## Prevention / Guardrail
- Keep preview pose math, source texture mapping, and saved raster presentation as separate named steps.
- Do not fix azimuth or mirror bugs by changing constants unless the coordinate contract being changed is explicit.
- Maintain a golden quaternion test against local Three.js for the `YXZ` pose conversion.
- Maintain a test for the viewer source texture mapping.
- Maintain a test that the final export raster flip is applied exactly once.
- When export and preview disagree, test the center ray numerically before changing yaw offsets.

## Follow-up: Nadir Cap
Date: 2026-09-01

The nadir-cap preview uses viewer texture space, while exported PNG and Stellarium rasters use the finalized South-centered export presentation. The export path therefore applies the cap after final rasterization and samples the cap texture in the saved-raster convention, including the required 180° texture rotation so the label reads upright when looking South.

## References
- Commit:
- ADR:
- CI Log:
- Code: `src-tauri/src/lib.rs`
- Code: `crates/panopose-core/src/export.rs`
- Code: `frontend/src/main.ts`

## Tags
- panorama-export
- coordinate-systems
- threejs
- skyseg
- stellarium
