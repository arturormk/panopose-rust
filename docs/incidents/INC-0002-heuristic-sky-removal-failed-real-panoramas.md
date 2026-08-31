# INC-0002-heuristic-sky-removal-failed-real-panoramas
Status: Closed
Date: 2026-08-31

## Context / Trigger
PanoPose needed an option to remove sky from panorama exports so the resulting landscape textures could work cleanly in Stellarium. The first implementation attempted to detect sky directly in the app with a traditional image-processing algorithm and a user-facing "Sky sensitivity" control.

## Symptom
The heuristic sky removal was unreliable on real panorama images:

- Blue sky was not consistently separated from haze, clouds, branches, and bright ground objects.
- Tree lines and leaf gaps produced visible halos or leftover sky fragments.
- Adjusting sensitivity improved one part of an image while making another part worse.
- The UI exposed an implementation detail, "Sky sensitivity", without giving users a stable way to get a good result.
- Follow-up cleanup passes could reduce edge contamination but could not solve incorrect sky classification.

## Root Cause
The problem was semantic segmentation, not color thresholding. Sky in real panoramas varies by exposure, lens correction, clouds, time of day, foliage occlusion, and compression artifacts. A classical algorithm based on color, gradients, connected regions, or local cleanup does not have enough scene understanding to distinguish sky from visually similar non-sky regions, especially around trees and horizon clutter.

The cleanup pass that removed blue spill from partial-alpha edge pixels was useful only after a good mask existed. It could not compensate for a bad mask because missed sky pixels were never marked as sky in the first place.

## Resolution
The in-house heuristic implementation and "Sky sensitivity" control were removed. PanoPose now detects whether `skyseg-ncnn` is available on `PATH`; when present, the app shows the "Remove Sky" toggle.

When sky removal is requested, PanoPose exports the pose-corrected panorama, runs:

```text
skyseg-ncnn <corrected-pano-image> <mask.jpg>
```

and uses the generated mask as sky opacity input, with black meaning ground, white meaning sky, and gray meaning mixed edge pixels. The mask is converted to an alpha mask, remapped when needed, and combined with a final edge decontamination pass for partial-alpha sky spill.

The selected external implementation is `knyipab/skyseg-ncnn`, a CMake-packaged build of `xiongzhu666/Sky-Segmentation-and-Post-processing` using OpenCV and ncnn.

## Prevention / Guardrail
- Treat sky removal as semantic segmentation unless the input domain is tightly controlled.
- Do not expose tuning sliders for unreliable algorithms as a substitute for correct classification.
- Keep sky segmentation optional and capability-driven: only show "Remove Sky" when `skyseg-ncnn` is found on `PATH`.
- Keep the segmentation mask polarity explicit in tests: skyseg white is sky, exported alpha white is opaque ground.
- Keep edge decontamination scoped to partial-alpha cleanup; do not rely on it to fix missed sky classification.
- If another segmentation backend is added, preserve the same app contract: corrected input image in, grayscale sky mask out.

## References
- Commit:
- ADR:
- CI Log:
- Project: https://github.com/knyipab/skyseg-ncnn
- Upstream model source: https://github.com/xiongzhu666/Sky-Segmentation-and-Post-processing
- Code: `src-tauri/src/lib.rs`
- Code: `frontend/src/main.ts`

## Tags
- sky-removal
- semantic-segmentation
- image-processing
- skyseg
- stellarium
