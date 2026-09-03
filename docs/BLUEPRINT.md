# BLUEPRINT.md

# PanoPose

Repository/project directory: `panopose-rust`

PanoPose is a Linux desktop application for manually orienting, registering, inspecting, and exporting full-sphere equirectangular panoramas.

The intended images cover the entire sphere:

* 360° horizontally
* 180° vertically
* normally stored as 2:1 equirectangular images
* arbitrary practical resolutions

The primary use case is astronomical observing-site photography. A user captures a spherical panorama from a particular observing position and then calibrates that panorama so every direction in the image corresponds accurately to a real-world Alt/Az direction.

Once calibrated, the panorama can serve as:

* a reference for registering later panoramas from the same location;
* a landscape reference for astronomical planning;
* a way to visualize where the Sun, Moon, planets, or stars appear relative to terrain and other horizon features;
* a source for a normalized equirectangular export;
* optionally, a landscape image with sky removed to transparency.

---

# Important interpretation of this blueprint

This document is **descriptive, not prescriptive**.

It records the initial concept, intended workflows, current architectural thinking, and design constraints for PanoPose. It is meant to give an AI coding agent such as Codex enough context to understand what the project is trying to accomplish.

It is **not** a requirement that every implementation decision described below be followed literally.

During development, better solutions may become apparent. Libraries may prove unsuitable. User-interface ideas may work differently in practice than expected. A simpler or more robust architecture may emerge.

The implementation may therefore deviate from this blueprint whenever doing so makes PanoPose:

* better;
* simpler;
* safer;
* faster;
* easier to maintain;
* easier to use;
* more accurate;
* or more useful.

The important requirement is to preserve the underlying goals and conceptual integrity of the application, not to mechanically reproduce every implementation detail suggested here.

This initial blueprint should remain in the repository as a record of the original design direction even if the implementation later evolves beyond it.

## Current implementation snapshot

As of the current implementation, PanoPose is a working Tauri + Rust + TypeScript/Three.js desktop app with:

* target panorama loading and spherical viewing;
* separate navigation and `Align Target` modes;
* numeric yaw/azimuth, pitch/altitude, and roll/horizon-tilt controls;
* reference panorama layers with opacity, target-only, reference-only, blend, and blink comparison modes;
* a zoom-dependent Alt/Az grid;
* EXIF/XMP metadata reading for time, timezone offset, GPS position, elevation, GPano pose, and PanoPose metadata;
* site fields that stay blank when no geolocation metadata is found, with latitude/longitude entry accepting decimal point or decimal comma input and imported coordinates rounded to five decimal places;
* site-dependent astronomy controls that are hidden and disabled until latitude and longitude are present, then refreshed automatically when the user completes the fields, with blank elevation treated as 0 m for astronomy calculations;
* metadata-only import from ordinary image files for reference time and/or site;
* session-only remembered image directory for image-open dialogs;
* metadata writing via `exiftool`, including PanoPose JSON, GPano orientation tags, site/capture-time fields, and nadir-cap settings;
* PNG export of the currently oriented target panorama at original target resolution;
* export centered on South/180° azimuth with the horizon vertically centered;
* export overwrite confirmation and a modal progress bar driven by backend row-progress events;
* an optional branded black nadir cap at altitude -90° with interactive radius preview and PNG/Stellarium export burn-in;
* automatic, non-destructive sky-removal preview for the target panorama;
* cached sky-removal preview reuse while the target image and alignment angles are unchanged;
* `skyseg-ncnn`-based sky removal when the external executable is available on `PATH`;
* PNG export with sky removed to alpha when `Remove Sky` is enabled;
* a configured square RGBA application icon for Tauri release bundles;
* a root `quickstart-panopose.sh` helper for dependency setup, current-platform release builds, and optional app installation;
* a root `quickstart-skyseg-ncnn.sh` helper for guided Linux builds of the optional external `skyseg-ncnn` dependency under ignored `./thirdparty/` checkouts;
* Sun, Moon, planet, and selected bright-star markers rendered as open circles with labels below;
* a `Planetarium` toggle that renders the 1500 brightest bundled catalog stars above the horizon;
* automatic Planetarium enabling when imported EXIF time places the Sun below the horizon;
* a generated bright-star catalog subset bundled from the Yale Bright Star Catalog / NASA HEASARC Bright Star Catalog.

Known intentional simplifications:

* ordinary rectilinear photos are not projected into the spherical viewer;
* Planetarium stars are unlabeled and do not include constellation lines;
* proper-motion correction, atmospheric extinction, daily Sun/Moon tracks, saved sightings, horizon-profile import, richer project management, and manual sky-mask editing remain future work.

---

# 1. Core concept

A PanoPose panorama should not fundamentally be treated as a flat 2:1 bitmap that is translated or rotated in two dimensions.

It represents a **texture over a sphere**.

Calibration therefore means determining the panorama's orientation in real-world three-dimensional space.

The fundamental transform should be represented internally as a 3-D rotation, preferably using a quaternion or equivalent robust representation.

The UI may expose more intuitive values such as:

* azimuth/yaw;
* altitude/pitch;
* roll.

However, Euler angles do not need to be the authoritative internal representation.

The calibrated panorama ultimately maps every source pixel to a real-world Alt/Az direction.

---

# 2. Coordinate convention

PanoPose should use a clear real-world horizontal coordinate system.

Suggested convention:

* Azimuth 0° = North
* Azimuth 90° = East
* Azimuth 180° = South
* Azimuth 270° = West
* Altitude +90° = Zenith
* Altitude 0° = Geometric horizon
* Altitude -90° = Nadir

Azimuth should refer to **true/geographic north**, not magnetic north.

Any manually entered bearings derived from Google Earth, terrain data, or similar sources should therefore normally be interpreted as true azimuths.

---

# 3. Input format

The principal supported input is a full-sphere equirectangular panorama.

Typical properties:

* width:height ratio approximately 2:1;
* covers 360° horizontally;
* covers 180° vertically;
* may come from a 360 camera;
* may come from stitched conventional photographs;
* may have almost any practical pixel resolution.

Examples might include:

* 6000×3000
* 8000×4000
* 12000×6000

The program should validate that an imported image plausibly represents a 2:1 full-sphere equirectangular panorama.

Slight deviations caused by metadata, padding, or unusual exporters may eventually warrant tolerant handling rather than a hard rejection.

---

# 4. Interactive workspace

PanoPose should have one principal interactive representation:

## Zoomable spherical view

The user navigates inside or around a spherical rendering of the panorama.

There is intentionally no requirement for a separate editable rectangular/equirectangular view.

The equirectangular representation is primarily:

* an import format;
* an export format;
* an internal texture representation.

All serious orientation work should happen in the spherical view.

The viewer should support:

* mouse/touchpad navigation;
* zooming;
* high magnification for fine registration;
* reference overlays;
* multiple raster panoramas where applicable;
* an optional Alt/Az grid.

The viewer should remain useful at very high zoom because fine manual alignment is a primary workflow.

---

# 5. Viewer navigation versus panorama adjustment

A critical usability requirement is to distinguish between:

1. moving the viewer;
2. moving the panorama.

Normal dragging should ordinarily change where the user is looking.

Changing the actual calibrated pose of the target panorama should require an explicit mode, modifier, or clearly visible control state.

For example:

* `Navigate`
* `Align Target`

The exact UI is open to experimentation, but accidental modification of a calibrated panorama while merely trying to look around should be difficult.

Current implementation note:

* `Align Target` dragging moves the panorama in the same direction as the pointer, including vertical drag for roll/horizon-tilt adjustment.

Undo/redo should apply to pose adjustments.

---

# 6. Manual alignment philosophy

PanoPose is primarily intended as a **manual precision-registration tool**.

Automatic computer-vision registration is not a requirement.

The expected workflow is:

* overlay a reference;
* zoom into recognizable detail;
* visually compare;
* nudge the target orientation;
* inspect another region;
* repeat until satisfactory.

The application should favor transparency and user control over opaque automatic alignment.

Future automatic assistance is acceptable if useful, but it should not dictate the initial architecture.

---

# 7. Fine orientation controls

The user should be able to adjust the target panorama's orientation numerically.

The UI should expose at least:

* azimuth/yaw;
* altitude/pitch;
* roll.

Fine `+` and `-` controls are particularly important for roll.

A useful initial increment scheme might include:

* 1°
* 0.1°
* 0.01°
* possibly 0.001°

A default fine adjustment of approximately 0.01° is expected to be useful.

Keyboard nudging should be considered.

For example, the implementation might eventually provide:

* directional keys for yaw/pitch;
* keys such as Q/E for roll;
* modifiers for coarse/fine increments.

The specific bindings are not mandated.

---

# 8. Alt/Az grid

The spherical viewer should optionally display a true Alt/Az grid.

The grid should include:

* horizon;
* altitude lines;
* azimuth meridians;
* cardinal directions;
* numeric labels where useful.

The grid should use **automatic level of detail** based on angular scale on screen.

At wide zoom it might show coarse intervals such as 30°.

At greater magnification it might progressively show:

* 10°
* 5°
* 2°
* 1°
* finer values where practical.

The goal is to keep the grid useful rather than cluttered.

The selection of spacing should preferably depend on projected screen spacing, not source-image resolution.

---

# 9. Raster reference panoramas

A common workflow is to align a new panorama against an older panorama that is already known to be well calibrated.

PanoPose should support loading at least:

* one calibrated reference panorama;
* one target panorama being adjusted.

Possible comparison modes include:

* target only;
* reference only;
* opacity blend;
* rapid blink/toggle;
* possibly wipe/split comparison later.

The two panoramas may have different resolutions.

Because both represent spherical data, comparison should be performed in spherical coordinates rather than by naïvely matching flat pixel coordinates.

Repeated interactive adjustments should not repeatedly resample or degrade the original image.

The transform should remain mathematical until final export.

---

# 10. Calibration state

A panorama may conceptually be:

* uncalibrated;
* approximately calibrated;
* calibrated/trusted.

This does not necessarily need to become a complicated formal state machine.

However, the project model should make it possible to know which panoramas are considered reliable references.

A calibrated panorama can itself become a reference for later panoramas.

Optional provenance metadata may be useful, for example:

* manually aligned to panorama X;
* aligned using Sun and terrain;
* aligned using PeakFinder horizon;
* aligned using astronomical night reference.

This is mainly for human understanding rather than mathematical necessity.

---

# 11. Observing sites and viewpoints

PanoPose should distinguish between a broad **site** and a precise **viewpoint**.

Example:

Site:

`Mountain Summit`

Viewpoint:

`Beside summit cairn`

A panorama belongs to a viewpoint.

This matters because moving even tens of meters can change:

* nearby terrain;
* trees;
* buildings;
* foreground obstructions;
* apparent skyline.

A site may eventually contain multiple viewpoints.

A viewpoint may contain multiple panoramas captured at different times.

---

# 12. Site information

Astronomical overlays require observer information.

A site/viewpoint should support at least:

* latitude;
* longitude;
* elevation;
* timezone.

The timezone should preferably use an unambiguous identifier such as an IANA timezone where practical.

Coordinates may come from:

* EXIF GPS metadata;
* manual entry;
* copied coordinates from mapping software;
* future import mechanisms.

The program should work offline once necessary site and astronomical data are locally available.

Offline usability is especially important because one target use case involves remote mountains or sites with poor connectivity.

---

# 13. Two principal real-world use cases

## 13.1 Home-base panorama

The user has a regular observing location such as a home terrace, garden, observatory, or nearby field.

The user can afford to establish a particularly accurate calibrated panorama.

Possible workflow:

1. Capture one or more panoramas.
2. Use astronomical references, terrain references, or a night panorama to establish accurate orientation.
3. Save a trusted daytime panorama as the master reference.
4. Later, capture updated panoramas whenever the landscape changes.
5. Manually align new panoramas against the trusted master.
6. Use the calibrated panorama for astronomical planning.

A night panorama may serve as an intermediate calibration reference even if its image quality is poor.

Example:

astronomical sky
→ nighttime panorama
→ high-quality daytime panorama
→ future daytime panoramas

---

## 13.2 Travel / remote-site survey

The user casually visits a promising astronomical site that may be difficult or time-consuming to reach again.

Examples:

* mountain summit;
* remote viewpoint;
* rural dark-sky site;
* location several hours from home.

The first visit may not be a formal observing session.

The user might only:

* capture a 360 panorama;
* record location;
* take a few ordinary photographs;
* perhaps photograph the Sun, Moon, or another object;
* then leave.

Later at home, the user calibrates the panorama.

The goal is:

**survey now, calibrate later, plan the return visit.**

Once calibrated, the user can study future celestial events relative to the landscape before returning.

---

# 14. Panorama capture time

PanoPose should attempt to read useful EXIF metadata where available.

Possible metadata includes:

* capture date;
* capture time;
* timezone/offset;
* GPS location.

EXIF timestamps should be treated as useful input, not unquestionable truth.

Timezone metadata is often missing or ambiguous.

The UI should therefore allow manual correction or replacement.

The program should present interpreted time clearly enough that the user can detect an incorrect timezone assumption.

For example:

`2026-08-29 18:37 Europe/Madrid`

and optionally the corresponding UTC instant.

Current implementation note:

* target panorama loading applies useful EXIF/XMP metadata automatically and initializes a separate panorama capture-time field;
* imported latitude and longitude values are rounded to five decimal places before display;
* latitude, longitude, and elevation fields stay blank when the loaded image has no site metadata;
* latitude and longitude text fields accept either decimal point or decimal comma input;
* opening a target without latitude/longitude metadata shows a warning and disables site-dependent astronomy controls;
* `Load EXIF from Image` allows importing reference time, site, or both from an ordinary photo without adding that photo as a layer;
* saving writes EXIF capture-time tags from the panorama capture-time field, not from externally imported reference time unless the user deliberately copies it;
* metadata saving deletes EXIF GPS tags when latitude or longitude is blank, but can write EXIF GPS latitude/longitude without GPS altitude when elevation is blank;
* if the imported time implies that the Sun is below the horizon, PanoPose enables Planetarium mode automatically.

---

# 15. Astronomical reference time

The astronomical simulation time must be **independent of the loaded panorama's capture time**.

This is important.

The user may have:

* a daytime 360 panorama;
* an ordinary night photograph from another date;
* an external photograph showing the Moon or Jupiter near the horizon;
* a known astronomical event.

The astronomical overlay should therefore support arbitrary manually selected dates and times without modifying the panorama metadata.

Possible time modes include:

### Panorama time

Use the loaded panorama's EXIF/manual capture time.

### Reference time

Use a separately entered date/time associated with some external observation.

### Explore time

Choose a date and interactively scrub through the day/night until a simulated celestial object visually matches an external photograph.

These exact mode names are suggestions rather than requirements.

Current implementation note:

* the app exposes a manually editable reference time and timezone field;
* external photo support is metadata-only, keeping PanoPose out of normal-photo projection and lens calibration work.

---

# 16. Astronomical overlays

PanoPose should eventually be able to overlay astronomical objects directly on the spherical view.

Desired objects include:

* Sun;
* Moon;
* bright planets;
* bright stars.

The astronomical layer should live in true Alt/Az space and remain fixed while the panorama is being adjusted.

The first implementation does not need to reproduce Stellarium visually.

Simple but accurate reference markers are sufficient.

Current implementation note:

* major objects are rendered as open circle markers with labels below the object so the underlying panorama remains visible;
* supported major markers currently include Sun, Moon, Mercury, Venus, Mars, Jupiter, Saturn, Vega, and Sirius from the frontend request list;
* Planetarium mode renders an unlabeled field of the 1500 brightest catalog stars above the horizon, with brighter stars drawn larger/brighter.

Potential future display features include:

* automatic magnitude limit based on zoom;
* labels for selected objects or stars;
* constellation lines;
* Moon phase;
* actual approximate apparent angular diameter of Sun/Moon.

These are enhancements, not core requirements.

---

# 17. Solar and lunar daily tracks

The user should be able to select a date and optionally display the daily track of:

* the Sun;
* the Moon.

The track should be rendered on the spherical viewer in Alt/Az coordinates.

Possible enhancements:

* above-horizon portion emphasized;
* below-horizon portion dimmed or dashed;
* hourly ticks;
* time labels;
* sunrise/set markers;
* moonrise/set markers;
* marker for selected time.

The user should be able to move through time and see the active Sun/Moon marker travel along the track.

---

# 18. Time exploration for external photographs

An external astronomical photograph does not need to be imported into PanoPose.

The user may have it open:

* in another application;
* on another monitor;
* on a phone/tablet;
* on another device.

Suppose the user knows only the date of a photograph showing the Moon near a mountain ridge.

The workflow can be:

1. Set the correct site.
2. Set the known date.
3. Enable the Moon.
4. Scrub the simulation time.
5. Watch the Moon move across the spherical panorama.
6. Stop when the simulated Moon occupies the same position relative to terrain as in the external photo.
7. Use that correspondence to refine panorama orientation.

This turns an imprecisely timestamped external photograph into a useful manual calibration reference.

Time controls should therefore be convenient.

Possible increments:

* hours;
* 10 minutes;
* 1 minute;
* 10 seconds.

A continuous slider/scrubber may also be useful.

---

# 19. Saved astronomical sightings

PanoPose may optionally allow the user to save named reference times/sightings.

Example:

`Moon over western ridge`

with:

* date;
* estimated or exact time;
* timezone;
* celestial object;
* notes.

No photograph needs to be stored.

A saved sighting might optionally allow the viewer to jump back to:

* the stored time;
* the stored object;
* approximately the relevant viewing direction.

This is convenient rather than essential.

---

# 20. Sun-based calibration

A common travel workflow may involve a daytime panorama containing the Sun.

If the site and capture time are known, PanoPose can calculate the Sun's Alt/Az position.

The user can then manually adjust the panorama until the photographed Sun coincides with the simulated Sun.

A single celestial point does not uniquely determine the complete 3-D orientation, so another constraint may still be needed, especially for roll.

Possible additional constraints include:

* known azimuth of a landmark;
* terrain horizon;
* Moon;
* stars;
* another astronomical sighting;
* approximate assumption that camera "up" was near vertical.

---

# 21. Moon, planets, and stars as references

Moon, planets, and stars can provide powerful nighttime/twilight calibration references.

A deliberately captured night panorama can be low quality as long as:

* stars/planets are visible;
* enough landscape detail remains recognizable.

Such an image can become a bridge between astronomical truth and a high-quality daytime panorama.

Example:

1. Generate/display a known-good astronomical sky.
2. Align a nighttime real panorama to the stars/planets.
3. Treat the night panorama as calibrated.
4. Align a daytime panorama to the night panorama using terrain.
5. Treat the daytime panorama as the durable high-quality reference.

Current implementation note:

* Planetarium mode already supports this workflow by drawing the brightest 1500 catalog stars above the horizon for the selected site/time;
* a normal phone photograph can provide reference time and site through EXIF import while remaining outside the rendering pipeline;
* the user can compare that external photo visually against PanoPose's star field and terrain panorama.

---

# 22. Stellarium references

PanoPose should remain compatible with workflows involving Stellarium.

A Stellarium render may be used as an externally generated known-good astronomical reference.

If practical, PanoPose may support importing a 2:1 equirectangular Stellarium render that has a known orientation.

The expected canonical convention would likely be:

* horizon at vertical center;
* known center azimuth;
* preferably 180° South by default.

A documented Stellarium export recipe or script may eventually be useful.

However, once PanoPose can render stars, planets, Sun, and Moon directly, importing a Stellarium raster should be considered an optional interoperability feature rather than a mandatory dependency.

Stellarium can also serve as an independent validation source during development.

Current implementation note:

* PanoPose now renders Sun, Moon, planets, selected bright stars, and a dense unlabeled bright-star field directly;
* Stellarium export/import remains optional future interoperability rather than a required calibration step.

---

# 23. Manual landmark azimuth references

The user may know the true azimuth of a visible landmark without knowing its altitude.

Example sources:

* Google Earth;
* GIS data;
* maps;
* measured bearings corrected to true north.

PanoPose should allow the user to create a named known-azimuth reference.

Example:

`Church tower — 247.36°`

Because altitude is unknown, this reference should appear as a vertical Alt/Az meridian at that azimuth.

The user manually moves the panorama until the chosen landmark lies on that meridian.

Multiple landmark azimuths spread around the horizon can help validate and refine orientation.

This feature is particularly useful for remote-site panoramas where only one daytime 360 image exists.

---

# 24. Imported terrain/horizon references

PanoPose should support, or be architected so it can reasonably support, imported terrain horizon profiles.

An ideal representation is a set of:

`azimuth → altitude`

samples.

Such a profile can be rendered as a precise line over the spherical panorama.

The user can manually align visible mountain peaks and valleys with the synthetic reference horizon.

Potential sources include:

* PeakFinder;
* Stellarium landscape/horizon formats;
* custom tools;
* future PanoPose-compatible generators.

A polygonal or sampled horizon reference is often superior to a raster screenshot because it is:

* compact;
* precise;
* resolution independent;
* easy to render at arbitrary zoom;
* directly expressed in Alt/Az coordinates.

---

# 25. PeakFinder interoperability

PeakFinder is a useful source of calibrated terrain geometry.

The user already has experience with a separate tool that creates material from PeakFinder for Stellarium.

PanoPose should therefore avoid unnecessarily inventing an incompatible terrain representation if existing Stellarium/PeakFinder formats can be parsed reliably.

Possible future capability:

`Import PeakFinder/Stellarium horizon`

The exact format and parser should be investigated during implementation.

The blueprint does not mandate a particular external format if a better practical solution becomes apparent.

---

# 26. Generic reference-overlay model

PanoPose should conceptually treat reference information as layers in fixed real-world Alt/Az space.

Potential layers include:

* Alt/Az grid;
* Sun;
* Moon;
* planets;
* stars;
* Sun track;
* Moon track;
* calibrated panorama;
* synthetic Stellarium panorama;
* known-azimuth landmark meridians;
* imported terrain/horizon profile.

The panorama being calibrated is manipulated relative to these fixed layers.

This generic model should be preferred over building many unrelated calibration modes.

---

# 27. Astronomy accuracy

Astronomical positions should be accurate enough for panorama registration and horizon-level observational planning.

Important considerations include:

* observer latitude/longitude;
* elevation;
* UTC instant;
* topocentric rather than merely geocentric coordinates where relevant;
* especially lunar parallax;
* atmospheric refraction near the horizon.

The implementation should investigate suitable Rust astronomy/ephemeris libraries.

The project should not tightly couple its entire data model to one library.

An internal astronomical/ephemeris abstraction may be preferable so the backend implementation can change later.

Accuracy should be verified against trusted external software such as Stellarium.

---

# 28. Refraction

The distinction between geometric and apparent altitude matters near the horizon.

Where practical, PanoPose should be able to use apparent topocentric positions, including atmospheric refraction.

The exact model does not need to be overengineered initially.

However, the implementation should avoid silently mixing:

* geometric horizon coordinates;
* refracted apparent coordinates.

If both become available, the UI or internal model should remain clear about which is being displayed.

---

# 29. Export

The user should be able to export a transformed full-sphere equirectangular image.

The export should normally:

* remain 2:1;
* cover 360×180°;
* place altitude 0° at the vertical center;
* apply the calibrated panorama orientation;
* perform one high-quality resampling from the original source.

The user should be able to choose output resolution.

The original image should remain untouched.

Current implementation:

* `Export Pano As` exports the current target as PNG only, preserving the target image's original dimensions;
* South/180° is the fixed export center in the UI;
* `Export Stellarium ZIP` bakes the current target pose into the texture and keeps Stellarium's `angle_rotatez` at the fixed spherical-texture convention;
* an optional `Nadir Cap` toggle burns a black branded cap into `Export Pano As` and `Export Stellarium ZIP` output after final raster orientation, with text readable upright when looking South;
* existing output files require confirmation before overwrite;
* export runs in Rust on Tauri's blocking thread pool and reports progress by completed output rows.

---

# 30. Configurable export center azimuth

A full sphere has no intrinsically correct horizontal seam.

PanoPose should therefore allow the user to choose the central azimuth of the exported equirectangular image.

Default:

`180° — South`

With center azimuth 180°:

* South appears at the center;
* North lies at the joined left/right seam.

Other values should be allowed.

The default may be stored as a project/application preference.

Vertical center should normally remain the horizon.

Current implementation note:

* the Rust core and CLI support arbitrary export center azimuth;
* the desktop UI currently uses the default South-centered export and does not expose this as a user setting.

---

# 31. No cumulative resampling

Interactive orientation changes should never repeatedly resample the source image.

The source panorama should remain unchanged.

During editing, only orientation metadata changes.

The GPU renderer can sample the original texture using the current transformation.

Final export should perform a single resampling operation from the original source into canonical output coordinates.

This avoids progressive image degradation.

---

# 32. Large-image handling

Panoramas may be large enough to exceed practical GPU texture limits or create excessive memory use.

The architecture should separate:

* interactive preview;
* full-resolution source;
* final export.

A practical implementation might:

* decode the full source in the backend;
* create a suitable downsampled preview for interactive WebGL rendering;
* preserve the original full-resolution image;
* perform full-resolution export in Rust.

The exact mechanism should be chosen based on performance testing.

Current implementation:

* interactive rendering uses the loaded texture directly in Three.js;
* full-resolution export is performed in Rust;
* export progress is emitted at row intervals so the frontend can show a determinate progress bar without flooding the event channel.

---

# 33. Sky removal / transparency

PanoPose should include an easy way to remove blue sky from a panorama and export it as transparency.

This should be non-destructive.

The preferred conceptual model is an alpha/mask layer rather than changing source RGB pixels permanently.

The current implementation uses the external `skyseg-ncnn` executable when it is available on `PATH`.

Useful controls might include:

* click/sample blue sky;
* hue/color tolerance;
* saturation threshold;
* brightness threshold;
* edge feathering;
* connected-region selection;
* manual add/remove brush.

Special care is required because the left and right edges of an equirectangular image are adjacent on the sphere.

Selections and masks should therefore be seam-aware.

Current implementation:

* `Remove Sky` is shown only when `skyseg-ncnn` is available on `PATH`;
* preview/export renders the corrected panorama, runs `skyseg-ncnn <corrected-pano-image> <mask.jpg>`, applies the mask as inverted alpha, and subtracts local sky color from semi-transparent edge pixels;
* sky removal is non-destructive and only affects preview/export.

---

# 34. Connected-sky selection

A useful sky-removal feature is:

`Connected sky only`

The user selects a representative sky region.

PanoPose then removes matching regions connected to that area rather than every blue object in the image.

This helps preserve:

* blue signs;
* cars;
* pools;
* painted objects;
* other non-sky blue areas.

Manual correction tools should still be available.

Current implementation note:

* the detector works top-down from the zenith toward the horizon and stops after clear non-sky barriers;
* horizontal seam wrapping is included;
* lower sky-colored foreground objects are intended to remain opaque after a clear separation;
* clouds, Sun, or Moon may remain opaque when removing them would risk bleeding into foreground whites.

---

# 35. Horizon-assisted masking

Once the panorama is calibrated, PanoPose knows the geometric horizon.

Altitude may therefore be useful as an additional heuristic for sky segmentation.

It should not be treated as an absolute boundary because:

* mountains rise above 0°;
* buildings rise above 0°;
* trees rise above 0°.

But it may help guide or constrain automated selection.

This is an optional enhancement.

---

# 36. Mask export

Possible outputs should include:

* normal RGB panorama;
* RGBA panorama with transparent sky;
* grayscale mask/alpha image.

PNG is a natural transparency output format.

Other formats may be supported if convenient.

Current implementation:

* `Export Pano As` writes RGBA PNG output;
* when `Remove Sky` is enabled, the corrected panorama is segmented by `skyseg-ncnn` and the resulting mask becomes output alpha.

---

# 36.1. Nadir cap

The user may need to hide a tripod, monopod, or stitching artifact at the nadir.

Current implementation:

* `Nadir Cap` is an optional panorama setting with a live spherical preview;
* the radius spinner controls how many degrees away from altitude -90° the cap extends, from 1° to 45°;
* the cap is a black circular overlay with the PanoPose icon centered on the nadir and `Pano` / `Pose` text on either side;
* `Export Pano As` and `Export Stellarium ZIP` burn the cap into the exported pixels;
* `Save As` stores the enabled state and radius in PanoPose metadata but does not render the cap, remap the panorama, or transcode pixels.

---

# 37. Project file

PanoPose should use a non-destructive project format, probably JSON or another readable structured format.

A project may contain:

* project version;
* sites;
* viewpoints;
* panorama references;
* image paths;
* optional hashes or fingerprints;
* orientation quaternions;
* calibration/trust status;
* calibration provenance;
* EXIF interpretation;
* site coordinates;
* timezone;
* astronomical reference settings;
* saved sightings;
* known-azimuth landmarks;
* imported horizon references;
* sky-mask information;
* export preferences;
* useful UI state.

The exact schema should evolve during implementation.

Versioning should be considered from the beginning so future migrations remain manageable.

---

# 38. Image path handling

Projects should preferably reference original image files rather than duplicating them automatically.

The implementation should consider:

* relative paths;
* absolute paths;
* project portability;
* missing images;
* moved files;
* optional file hashes for identification.

The simplest robust solution is preferable initially.

---

# 39. Rust + Tauri architecture

The current preferred architecture is:

* Rust core/backend
* Tauri
* TypeScript frontend
* WebGL/Three.js or equivalent for interactive spherical visualization

The rationale is:

* Rust is suitable for image decoding, metadata, geometry, astronomy, project state, and export;
* browser/WebGL tooling is well suited to interactive spherical texture rendering;
* Tauri provides a lightweight Linux desktop shell;
* the same Rust core can potentially support future CLI tooling.

This architecture is **not mandatory** if implementation experience demonstrates a better approach.

---

# 40. Separation of core from GUI

Where practical, important application logic should live outside Tauri-specific GUI code.

Potential reusable core responsibilities:

* project model;
* coordinate conversions;
* quaternion/orientation logic;
* ephemeris interface;
* EXIF parsing;
* horizon-reference parsing;
* export/resampling;
* mask processing.

This would make future tools possible, for example:

`panopose export project.json panorama-name`

Such a CLI is optional but the architecture should not unnecessarily prevent it.

---

# 41. Frontend rendering

The frontend should use GPU rendering for the interactive sphere.

The exact renderer may be Three.js or another suitable WebGL abstraction.

The frontend should be responsible primarily for:

* viewer navigation;
* displaying panorama textures;
* blending layers;
* drawing grid/astronomy overlays;
* interactive controls;
* zoom;
* alignment visualization.

The Rust backend should remain responsible for authoritative project state and high-quality export where appropriate.

The exact boundary may evolve based on performance and implementation simplicity.

---

# 42. Development and release helper

The repository includes `quickstart-panopose.sh` as the practical release-build entrypoint.

Current behavior:

* checks local Rust, Node.js, and npm availability;
* offers to install `cargo-tauri` if it is missing;
* installs frontend dependencies with `npm ci --prefix frontend`;
* can force a clean Rust and frontend rebuild with `--force-rebuild`;
* asks which final packages to build using a numbered menu, unless `--bundles` or `--no-bundle` is provided;
* builds the `panopose-cli` release utility and a current-platform Tauri release with `cargo tauri build --ci`;
* lists generated bundle artifacts and the release executable paths;
* offers to install the preferred package where supported.

The Tauri bundle uses `src-tauri/icons/icon.png` as the release icon. The icon should remain a square RGBA PNG; the current asset is 512x512.

On Linux, install preference is:

* `.deb` via `sudo dpkg -i`;
* `.rpm` via `sudo rpm -Uvh`;
* `.AppImage` copied to `~/.local/bin/panopose.AppImage` with a desktop entry under `~/.local/share/applications`.

The Linux package prompt shows numbered options for `deb`, `rpm`, `appimage`, `deb,rpm`, `all`, or `none`, while still accepting typed names and comma-separated combinations. Linux `.deb` and `.rpm` packages include both `panopose` and `panopose-cli` under `/usr/bin`. If app installation is skipped or declined, the script prints absolute paths for the release executables and the requested final package files.

AppImage generation depends on Tauri's linuxdeploy/AppImage tooling and the host's AppImage/FUSE compatibility. The script may still leave usable `.deb` or `.rpm` artifacts when AppImage packaging fails on a constrained host.

The script intentionally does not cross-compile.

The repository also includes `quickstart-skyseg-ncnn.sh` as the practical Linux setup helper for the optional external sky removal dependency. It explains the external clone/build/install steps, asks for permission before starting, clones or reuses `Tencent/ncnn` and `knyipab/skyseg-ncnn` under ignored `./thirdparty/ncnn` and `./thirdparty/skyseg-ncnn` directories, defaults installation to `$HOME/.local`, and only offers the dated compatibility patches after an unpatched `skyseg-ncnn` build fails.

---

# 43. Orientation mathematics

The implementation should define one authoritative transformation convention early and test it thoroughly.

It should be possible to answer unambiguously:

* given a source equirectangular pixel, what sphere direction does it represent?
* given a calibrated orientation, what real Alt/Az direction does it represent?
* given a real Alt/Az direction, where should it be sampled from the source?
* where does that direction appear in a canonical export?

Coordinate-system confusion is likely to be one of the greatest technical risks.

Tests should cover:

* cardinal directions;
* zenith;
* nadir;
* horizon;
* seam crossing;
* roll;
* pole crossing;
* configurable export center.

---

# 44. Equirectangular mapping

A standard 2:1 equirectangular panorama maps longitude/azimuth horizontally and latitude/altitude vertically.

The implementation should explicitly document its chosen pixel-center and angular mapping conventions.

Off-by-one and half-pixel errors can matter at high resolution.

The same mapping convention should be used consistently between:

* viewer;
* overlays;
* reference panoramas;
* export;
* masks.

---

# 45. Precision

Orientation state should use sufficient floating-point precision.

Double precision may be appropriate for project geometry and astronomical calculations even if GPU rendering uses single precision.

Fine alignment increments such as 0.01° must be represented and preserved accurately.

Project serialization should not unnecessarily truncate orientation values.

---

# 46. Manual registration against an external image

An important workflow does not involve importing the reference photograph.

Example:

* the user has a normal photograph showing Venus just above a specific mountain;
* the exact or approximate date/time is known;
* the photograph is open elsewhere;
* PanoPose renders Venus for that date/time;
* the user adjusts the loaded 360 until the simulated Venus has the same relationship to the terrain.

This must not require PanoPose to understand:

* the external photo's lens;
* focal length;
* projection;
* image orientation;
* perspective.

The user performs the visual comparison manually.

Current implementation note:

* `Load EXIF from Image` imports only the external photo's time/site metadata;
* the app deliberately does not estimate field of view from incomplete phone EXIF or map ordinary photos onto the sphere.

---

# 47. Approximate-time external references

Even knowing only the date can be useful.

The user can move simulation time until the astronomical object visually reaches the location seen in the external photograph.

This is especially useful for:

* Moon;
* Sun;
* bright planets;
* bright stars.

PanoPose should describe any resulting inferred time as a manual estimate, not an automatically proven timestamp.

---

# 48. Multiple independent references

A user may combine several references during calibration.

Example:

* Sun position;
* church azimuth from Google Earth;
* mountain horizon from PeakFinder;
* Moon sighting from another evening.

PanoPose should allow these to coexist.

The application does not need to mathematically solve them automatically.

They serve as visual constraints against which the user manually optimizes panorama orientation.

---

# 49. Planning functionality

Once a panorama is calibrated, PanoPose naturally becomes useful for planning future observations.

The user may select a future date/time and see:

* Sun position;
* Moon position;
* planets;
* stars;
* daily tracks;
* their relationship to mountains, trees, buildings, and other obstructions.

This planning functionality should emerge from the same astronomical overlay system rather than becoming a separate application mode.

A concise description of the travel workflow is:

**I found a promising place. I took a 360. Now show me what the sky will do there.**

---

# 50. Scope discipline

PanoPose should resist becoming:

* a general panorama stitcher;
* a general photo editor;
* a full planetarium replacement;
* a GIS application;
* an automatic computer-vision registration suite.

Its core identity is:

**precision manual orientation and astronomical use of full-sphere panoramas.**

Features should be evaluated according to whether they strengthen that purpose.

---

# 51. Possible MVP

A useful initial milestone might include the items below. Many are now implemented, but the list remains useful as a compact description of the baseline product:

1. Linux Tauri application.
2. Load a 2:1 equirectangular image.
3. Render it as a zoomable spherical panorama.
4. Navigate independently of panorama adjustment.
5. Maintain panorama orientation as a quaternion.
6. Numeric yaw/pitch/roll controls.
7. Fine 0.01° nudging.
8. Alt/Az grid with cardinal directions.
9. Site coordinates and timezone.
10. Manual date/time selection.
11. Sun and Moon markers.
12. At least basic planets and bright-star support if practical.
13. Load a second panorama as a reference.
14. Opacity/blink comparison.
15. Save project state.
16. Export a calibrated 2:1 equirectangular image.
17. Configurable export-center azimuth, default 180° South.

Current implementation note:

* implemented: items 1-4, 6, 8-14, and 16 through the current Tauri/frontend/core workflow;
* partially represented: item 5 exists in the Rust core orientation model, while the frontend currently exposes yaw/pitch/roll directly;
* partially represented: item 17 exists in core/CLI export options, while the desktop UI currently exports with fixed South-centered output;
* not yet implemented as a full user workflow: item 15 project save/load.

Manual sky-mask editing and richer astronomy can be implemented immediately if convenient, or added as subsequent milestones.

---

# 52. Suggested subsequent features

After the basic orientation workflow is reliable:

* Sun/Moon daily tracks;
* time scrubbing;
* saved astronomical sightings;
* manually entered known-azimuth landmarks;
* PeakFinder/Stellarium horizon import;
* manual sky-mask editing;
* richer star/planet overlays beyond the current major markers and 1500-star Planetarium mode;
* large-image preview optimization;
* Stellarium equirectangular-reference import;
* richer CLI/project export workflows;
* better project/site management.

Again, this ordering is only a reasonable starting point.

---

# 53. Testing priorities

The most important automated tests are likely mathematical rather than UI tests.

Test:

* source pixel ↔ spherical direction;
* spherical direction ↔ Alt/Az;
* quaternion transforms;
* identity orientation;
* known yaw/pitch/roll transforms;
* seam behavior;
* zenith/nadir behavior;
* canonical export;
* non-default export center;
* matching viewer and export conventions;
* astronomy against trusted reference values;
* project save/load round trips.

For visual integration tests, synthetic panoramas containing known grids/cardinal markers may be extremely useful.

---

# 54. Development validation panorama

Consider generating a synthetic 2:1 test panorama containing:

* labeled azimuth lines;
* labeled altitude lines;
* N/E/S/W;
* zenith;
* nadir;
* horizon;
* seam markers.

This can make rendering and transformation mistakes immediately obvious.

It may be more valuable during early development than testing with natural photographs alone.

---

# 55. Performance philosophy

Correctness and interaction quality matter more than premature optimization.

However, the application should feel responsive during:

* zooming;
* looking around;
* opacity blending;
* fine pose adjustment;
* time scrubbing.

Expensive full-resolution resampling should not occur during every interactive adjustment.

GPU previews plus deferred high-quality export are the expected solution unless a better implementation appears.

---

# 56. Data ownership and non-destructive behavior

PanoPose should avoid modifying original panorama files unless the user explicitly requests an export to the same path and such behavior is considered safe.

The project should principally store metadata and transformations.

Original photographs are source material.

Calibrated exports are derived artifacts.

---

# 57. Error handling

The application should fail clearly and usefully for cases such as:

* image is not approximately 2:1;
* unsupported format;
* missing image referenced by project;
* invalid EXIF time;
* timezone unknown;
* invalid coordinates;
* unavailable astronomy data;
* corrupt horizon reference;
* output path unwritable.

The user should not have to understand internal mathematical or Rust errors.

---

# 58. User trust

Because the purpose is precision alignment, PanoPose should avoid silently guessing where a guess could materially affect orientation.

Examples:

* unknown timezone;
* magnetic versus true bearing;
* questionable EXIF date;
* missing location;
* refraction assumptions.

When uncertainty matters, expose it.

The application should remain easy to use, but it should prefer an explicit unknown over a plausible-looking incorrect answer.

---

# 59. Possible terminology

Use terminology consistently.

Recommended terms:

* **Panorama** — a loaded full-sphere image.
* **Target panorama** — the panorama currently being adjusted.
* **Reference panorama** — a calibrated panorama used for comparison.
* **Site** — broader observing location.
* **Viewpoint** — precise camera position within a site.
* **Pose** — the panorama's 3-D orientation.
* **Calibration** — determining the panorama's pose in real Alt/Az coordinates.
* **Overlay** — fixed reference information drawn over the sphere.
* **Astronomical reference time** — time used for celestial simulation independent of panorama EXIF time.
* **Horizon profile** — terrain represented as altitude versus azimuth.
* **Export center azimuth** — azimuth placed at horizontal center of the equirectangular output.

---

# 60. Why the project is called PanoPose

The name reflects the actual mathematical problem.

The application is not primarily about creating panoramas.

It is about determining the **pose** of an already existing spherical panorama relative to the real world.

`PanoPose`

therefore describes the project more accurately than names centered merely on "360 images."

The GitHub/repository directory is:

`panopose-rust`

---

# 61. Final design principle

PanoPose should keep one simple mental model:

**The real Alt/Az sphere is fixed.**

The panorama is placed onto that sphere.

Astronomical objects, terrain references, grids, landmark bearings, and calibrated panoramas provide fixed references.

The user manually adjusts the panorama's pose until those references agree with what is visible in the photograph.

Once that agreement is satisfactory, the panorama is calibrated.

Everything else—planning, comparison, normalized export, astronomical tracks, and sky masking—builds naturally on top of that calibrated spherical relationship.
