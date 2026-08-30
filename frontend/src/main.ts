import "./style.css";
import exifr from "exifr";
import * as THREE from "three";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";

type Mode = "navigate" | "align";
type LayerKind = "target" | "reference";
type ComparisonMode = "blend" | "target" | "reference" | "blink";
type MetadataImportChoice = "time" | "site" | "both";

type CelestialObject =
  | "sun"
  | "moon"
  | "mercury"
  | "venus"
  | "mars"
  | "jupiter"
  | "saturn"
  | "vega"
  | "sirius";

interface CelestialMarker {
  object: CelestialObject;
  label: string;
  alt_az: {
    altitude_deg: number;
    azimuth_deg: number;
  };
  magnitude: number | null;
  accuracy_note: string;
}

interface StarMarker {
  hr: number;
  alt_az: {
    altitude_deg: number;
    azimuth_deg: number;
  };
  magnitude: number;
}

interface Pose {
  yaw: number;
  pitch: number;
  roll: number;
}

interface AltAzReadout {
  altitudeDeg: number;
  azimuthDeg: number;
}

interface LocalDateTimeParts {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
}

interface ExifDateTimeResult {
  localDateTime: string;
  offset: string | null;
}

interface CaptureTimeState {
  localDateTime: string;
  timezone: string;
  source: string;
}

interface ImageMetadataResult {
  timestamp: ExifDateTimeResult | null;
  latitude: number | null;
  longitude: number | null;
  elevation: number | null;
  pose: Partial<Pose> | null;
  timezone: string | null;
}

interface ExportProgress {
  completed_rows: number;
  total_rows: number;
}

interface SkyRemovalSettings {
  sensitivity: number;
}

interface SkyPreviewCacheKey {
  path: string;
  sensitivity: number;
  pose: Pose;
}

interface StellariumExportDetails {
  outputZip: string;
  directoryName: string;
  textureFilename: string;
  landscapeName: string;
  author: string;
  description: string;
}

interface PanoposeMetadata {
  yaw_deg: number | null;
  pitch_deg: number | null;
  roll_deg: number | null;
  latitude_deg: number | null;
  longitude_deg: number | null;
  elevation_m: number | null;
  capture_time: string | null;
  reference_time: string | null;
  timezone: string | null;
}

interface GpanoPoseMetadata {
  heading_deg: number | null;
  pitch_deg: number | null;
  roll_deg: number | null;
}

interface ImageLayer {
  id: string;
  kind: LayerKind;
  name: string;
  path: string;
  pose: Pose;
  opacity: number;
  visible: boolean;
  objectUrl: string;
  texture: THREE.Texture;
  material: THREE.MeshBasicMaterial;
  mesh: THREE.Mesh;
  skyPreviewObjectUrl: string | null;
  skyPreviewTexture: THREE.Texture | null;
  skyPreviewCacheKey: SkyPreviewCacheKey | null;
  dimensions: {
    width: number;
    height: number;
  };
  hasPoseMetadata: boolean;
}

const systemTimeZone = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
const MAX_IMAGE_LAYERS = 4;
const BLINK_INTERVAL_MS = 550;
const DEFAULT_POSE: Pose = { yaw: 0, pitch: 0, roll: 0 };

const state = {
  mode: "navigate" as Mode,
  pose: { yaw: 0, pitch: 0, roll: 0 } satisfies Pose,
  step: 0.01,
  comparisonMode: "blend" as ComparisonMode,
  blinkShowTarget: true,
  layers: [] as ImageLayer[],
  markers: [] as CelestialMarker[],
  starMarkers: [] as StarMarker[],
  planetariumMode: false,
  openedImagePath: "",
  savePath: "",
  lastImageDirectory: "",
  captureTime: {
    localDateTime: "2026-08-29T18:37",
    timezone: systemTimeZone,
    source: "Default",
  } satisfies CaptureTimeState,
  skyRemoval: {
    enabled: false,
    sensitivity: 0.55,
    previewRequestId: 0,
  },
};

const PANORAMA_BASE_YAW_DEG = -90;

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <main class="app">
    <aside class="sidebar">
      <div class="brand">
        <h1>PanoPose</h1>
        <span class="status" id="image-status">No panorama</span>
      </div>

      <section class="panel">
        <h2>Panorama</h2>
        <button id="open-target" class="button" type="button">Open Image</button>
        <button id="export-image" class="button" type="button">Export Pano As</button>
        <button id="export-stellarium" class="button" type="button">Export Stellarium ZIP</button>
        <button id="save-metadata-as" class="button" type="button">Save As</button>
        <label class="field">Save target
          <input id="save-path" type="text" readonly placeholder="No image opened" />
        </label>
        <label class="field">Panorama capture time
          <input id="capture-time" type="datetime-local" value="2026-08-29T18:37" />
        </label>
        <label class="field">Capture time zone
          <input id="capture-timezone" list="timezone-options" value="${systemTimeZone}" />
        </label>
        <button id="use-reference-time-for-capture" class="button secondary" type="button">Use Reference Time</button>
        <div class="metadata-source" id="capture-time-source">Capture time: default</div>
        <label class="switch-field">
          <span>Remove Sky</span>
          <input id="remove-sky" type="checkbox" />
          <span class="switch-track" aria-hidden="true">
            <span class="switch-thumb"></span>
          </span>
        </label>
        <label class="field">Sky sensitivity
          <input id="sky-sensitivity" type="range" min="-1" max="1" step="0.01" value="0.55" />
        </label>
      </section>

      <section class="panel">
        <h2>Layers</h2>
        <button id="add-reference" class="button" type="button">Add Reference</button>
        <div class="segmented segmented-four">
          <button id="compare-blend" class="icon-button active" type="button" aria-label="Blend" title="Blend">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="9" cy="12" r="6"></circle>
              <circle cx="15" cy="12" r="6"></circle>
            </svg>
          </button>
          <button id="compare-target" class="icon-button" type="button" aria-label="Target" title="Target">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="4" y="5" width="16" height="14" rx="2"></rect>
              <path d="M8 15l2.5-3 2 2.2 2.5-3.2 3 4"></path>
              <circle cx="9" cy="9" r="1.2"></circle>
            </svg>
          </button>
          <button id="compare-reference" class="icon-button" type="button" aria-label="Reference" title="Reference">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="5" y="4" width="14" height="16" rx="2"></rect>
              <path d="M9 8h6"></path>
              <path d="M9 12h6"></path>
              <path d="M9 16h4"></path>
            </svg>
          </button>
          <button id="compare-blink" class="icon-button" type="button" aria-label="Blink" title="Blink">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M7 7h10v10H7z"></path>
              <path d="M4 4h10"></path>
              <path d="M4 4v10"></path>
              <path d="M20 20H10"></path>
              <path d="M20 20V10"></path>
            </svg>
          </button>
        </div>
        <div class="layer-list" id="layer-list">
          <div class="empty-layers">Open a target image or add a reference.</div>
        </div>
      </section>

      <section class="panel">
        <h2>Mode</h2>
        <div class="segmented">
          <button id="mode-navigate" class="active" type="button">Navigate</button>
          <button id="mode-align" type="button">Align Target</button>
        </div>
      </section>

      <section class="panel">
        <h2>Orientation</h2>
        <label class="field">Step
          <select id="step">
            <option value="1">1 deg</option>
            <option value="0.1">0.1 deg</option>
            <option value="0.01" selected>0.01 deg</option>
            <option value="0.001">0.001 deg</option>
          </select>
        </label>
        <label class="field">Yaw (Azimuth)
          <input id="yaw" type="number" step="0.001" value="0" />
        </label>
        <label class="field">Pitch (Altitude)
          <input id="roll" type="number" step="0.001" value="0" />
        </label>
        <label class="field">Roll (Horizontal Tilt)
          <input id="pitch" type="number" step="0.001" value="0" />
        </label>
      </section>

      <section class="panel">
        <h2>Site & Time</h2>
        <label class="field">Latitude
          <input id="latitude" type="number" step="0.000001" value="40.4168" />
        </label>
        <label class="field">Longitude
          <input id="longitude" type="number" step="0.000001" value="-3.7038" />
        </label>
        <label class="field">Elevation m
          <input id="elevation" type="number" step="1" value="667" />
        </label>
        <label class="field">Reference time
          <input id="time" type="datetime-local" value="2026-08-29T18:37" />
        </label>
        <label class="field">Time zone
          <input id="timezone" list="timezone-options" value="${systemTimeZone}" />
        </label>
        <datalist id="timezone-options">
          <option value="UTC"></option>
          <option value="Europe/Madrid"></option>
          <option value="Europe/London"></option>
          <option value="America/New_York"></option>
          <option value="America/Los_Angeles"></option>
          <option value="Asia/Tokyo"></option>
          <option value="Australia/Sydney"></option>
        </datalist>
        <button id="load-exif-metadata" class="button" type="button">Load EXIF from Image</button>
        <label class="switch-field">
          <span>Planetarium</span>
          <input id="planetarium-mode" type="checkbox" />
          <span class="switch-track" aria-hidden="true">
            <span class="switch-thumb"></span>
          </span>
        </label>
        <button id="refresh-astro" class="button" type="button">Refresh Markers</button>
      </section>
      <div class="app-version" id="app-version">v0.1.0</div>
    </aside>
    <section class="viewer">
      <canvas id="viewer-canvas"></canvas>
      <div class="overlay" id="readout">Navigate with drag and wheel. Switch to Align Target before changing panorama pose.</div>
    </section>
    <div class="modal-backdrop" id="metadata-import-modal" hidden>
      <section class="metadata-modal" role="dialog" aria-modal="true" aria-labelledby="metadata-import-title">
        <h2 id="metadata-import-title">Load EXIF from Image</h2>
        <div class="metadata-summary" id="metadata-import-summary"></div>
        <div class="modal-actions">
          <button id="metadata-import-time" class="button" type="button">Import Time</button>
          <button id="metadata-import-site" class="button" type="button">Import Site</button>
          <button id="metadata-import-both" class="button" type="button">Import Both</button>
          <button id="metadata-import-cancel" class="button secondary" type="button">Cancel</button>
        </div>
      </section>
    </div>
    <div class="modal-backdrop" id="stellarium-export-modal" hidden>
      <section class="metadata-modal" role="dialog" aria-modal="true" aria-labelledby="stellarium-export-title">
        <h2 id="stellarium-export-title">Export Stellarium ZIP</h2>
        <label class="field">Landscape name
          <input id="stellarium-name" type="text" />
        </label>
        <label class="field">Author
          <input id="stellarium-author" type="text" />
        </label>
        <label class="field">Description
          <input id="stellarium-description" type="text" />
        </label>
        <label class="field">Directory in ZIP
          <input id="stellarium-directory" type="text" />
        </label>
        <label class="field">Panorama PNG
          <input id="stellarium-texture" type="text" />
        </label>
        <a class="external-link" href="https://stellarium.org/landscapes.html" target="_blank" rel="noreferrer">Stellarium landscape installation</a>
        <div class="modal-actions">
          <button id="stellarium-export-confirm" class="button" type="button">Export ZIP</button>
          <button id="stellarium-export-cancel" class="button secondary" type="button">Cancel</button>
        </div>
      </section>
    </div>
    <div class="modal-backdrop" id="export-progress-modal" hidden>
      <section class="metadata-modal export-progress-modal" role="status" aria-live="polite" aria-labelledby="export-progress-title">
        <h2 id="export-progress-title">Exporting Panorama</h2>
        <div class="export-progress-message" id="export-progress-message"></div>
        <progress class="export-progress-bar" id="export-progress-bar" max="1" value="0"></progress>
        <div class="export-progress-percent" id="export-progress-percent">0%</div>
      </section>
    </div>
  </main>
`;

const canvas = document.querySelector<HTMLCanvasElement>("#viewer-canvas")!;
const renderer = new THREE.WebGLRenderer({ antialias: true, canvas });
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
renderer.setClearColor(0x101214);

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(70, 1, 0.1, 2000);
camera.position.set(0, 0, 0.01);

const sphereGeometry = new THREE.SphereGeometry(500, 96, 48);
sphereGeometry.scale(-1, 1, 1);
const placeholderMaterial = new THREE.MeshBasicMaterial({ color: 0x353b43, side: THREE.DoubleSide });
const placeholderPanorama = new THREE.Mesh(sphereGeometry, placeholderMaterial);
placeholderPanorama.renderOrder = 0;
scene.add(placeholderPanorama);

const gridGroup = new THREE.Group();
gridGroup.renderOrder = 100;
scene.add(gridGroup);

const markerGroup = new THREE.Group();
markerGroup.renderOrder = 110;
scene.add(markerGroup);

const starGroup = new THREE.Group();
starGroup.renderOrder = 105;
scene.add(starGroup);

const cursorNdc = new THREE.Vector2();
const cursorRaycaster = new THREE.Raycaster();

let yawView = Math.PI;
let pitchView = 0;
let dragging = false;
let lastX = 0;
let lastY = 0;
let cursorAltAz: AltAzReadout | null = null;
let activeGridSpacingDeg = 0;
let blinkTimer: number | null = null;
let skyPreviewTimer: number | null = null;

function setMode(mode: Mode): void {
  state.mode = mode;
  document.querySelector("#mode-navigate")!.classList.toggle("active", mode === "navigate");
  document.querySelector("#mode-align")!.classList.toggle("active", mode === "align");
  updateReadout();
}

document.querySelector("#mode-navigate")!.addEventListener("click", () => setMode("navigate"));
document.querySelector("#mode-align")!.addEventListener("click", () => setMode("align"));

document.querySelector("#open-target")!.addEventListener("click", () => openTargetImage());
document.querySelector("#export-image")!.addEventListener("click", () => exportCurrentTargetImage());
document.querySelector("#export-stellarium")!.addEventListener("click", () => exportCurrentTargetStellariumZip());
document.querySelector("#save-metadata-as")!.addEventListener("click", () => saveMetadataAs());
document.querySelector("#add-reference")!.addEventListener("click", () => addReferenceImages());
document.querySelector("#load-exif-metadata")!.addEventListener("click", () => loadExifMetadataFromImage());
document.querySelector("#use-reference-time-for-capture")!.addEventListener("click", () => useReferenceTimeForCapture());
document.querySelector("#capture-time")!.addEventListener("change", () => syncCaptureTimeStateFromInputs("Edited"));
document.querySelector("#capture-timezone")!.addEventListener("change", () => syncCaptureTimeStateFromInputs("Edited"));
document.querySelector("#remove-sky")!.addEventListener("change", (event) => {
  state.skyRemoval.enabled = (event.target as HTMLInputElement).checked;
  void applySkyPreview();
});
document.querySelector("#sky-sensitivity")!.addEventListener("input", (event) => {
  state.skyRemoval.sensitivity = Number((event.target as HTMLInputElement).value);
  scheduleSkyPreview();
});
document.querySelector("#compare-blend")!.addEventListener("click", () => setComparisonMode("blend"));
document.querySelector("#compare-target")!.addEventListener("click", () => setComparisonMode("target"));
document.querySelector("#compare-reference")!.addEventListener("click", () => setComparisonMode("reference"));
document.querySelector("#compare-blink")!.addEventListener("click", () => setComparisonMode("blink"));
document.querySelector("#layer-list")!.addEventListener("input", handleLayerListInput);
document.querySelector("#layer-list")!.addEventListener("click", handleLayerListClick);

for (const id of ["yaw", "pitch", "roll"] as const) {
  document.querySelector<HTMLInputElement>(`#${id}`)!.addEventListener("change", (event) => {
    state.pose[id] = Number((event.target as HTMLInputElement).value);
    applyPose();
  });
}

document.querySelector<HTMLSelectElement>("#step")!.addEventListener("change", (event) => {
  state.step = Number((event.target as HTMLSelectElement).value);
  syncStepInputs();
});

document.querySelector("#refresh-astro")!.addEventListener("click", () => refreshAstronomy());
document.querySelector("#planetarium-mode")!.addEventListener("change", (event) => {
  state.planetariumMode = (event.target as HTMLInputElement).checked;
  if (!state.planetariumMode) {
    state.starMarkers = [];
    drawStars();
    updateReadout();
    return;
  }
  void refreshAstronomy();
});

canvas.addEventListener("pointerdown", (event) => {
  dragging = true;
  lastX = event.clientX;
  lastY = event.clientY;
  updateCursorAltAz(event);
  canvas.setPointerCapture(event.pointerId);
});

canvas.addEventListener("pointermove", (event) => {
  if (!dragging) {
    updateCursorAltAz(event);
    return;
  }

  const dx = event.clientX - lastX;
  const dy = event.clientY - lastY;
  lastX = event.clientX;
  lastY = event.clientY;

  if (state.mode === "navigate") {
    const dragScale = navigationDragScale();
    yawView += dx * dragScale.horizontalRadiansPerPixel;
    pitchView = THREE.MathUtils.clamp(
      pitchView + dy * dragScale.verticalRadiansPerPixel,
      -Math.PI / 2 + 0.02,
      Math.PI / 2 - 0.02,
    );
  } else {
    const dragScale = navigationDragScale();
    state.pose.yaw -= THREE.MathUtils.radToDeg(dx * dragScale.horizontalRadiansPerPixel);
    state.pose.roll += THREE.MathUtils.radToDeg(dy * dragScale.verticalRadiansPerPixel);
    syncPoseInputs();
    applyPose();
  }

  updateCursorAltAz(event);
});

canvas.addEventListener("pointerup", (event) => {
  dragging = false;
  updateCursorAltAz(event);
  canvas.releasePointerCapture(event.pointerId);
});

canvas.addEventListener("pointerleave", () => {
  cursorAltAz = null;
  updateReadout();
});

canvas.addEventListener("wheel", (event) => {
  event.preventDefault();
  camera.fov = THREE.MathUtils.clamp(camera.fov + event.deltaY * 0.03, 8, 100);
  camera.updateProjectionMatrix();
  updateGridForZoom();
});

window.addEventListener("resize", resize);
syncStepInputs();
applyPose();
renderLayerList();
updateLayerRendering();
resize();
updateGridForZoom();
syncCaptureTimeInputs();
void loadAppVersion();
animate();

async function loadAppVersion(): Promise<void> {
  const versionElement = document.querySelector("#app-version");
  if (!versionElement) return;

  try {
    versionElement.textContent = await invoke<string>("app_version");
  } catch {
    versionElement.textContent = "v0.1.0";
  }
}

function applyPose(): void {
  const target = getTargetLayer();
  if (target) {
    target.pose = { ...state.pose };
    applyLayerPose(target);
    if (state.skyRemoval.enabled) {
      scheduleSkyPreview();
    }
  }
  updateReadout();
}

function syncPoseInputs(): void {
  for (const id of ["yaw", "pitch", "roll"] as const) {
    document.querySelector<HTMLInputElement>(`#${id}`)!.value = state.pose[id].toFixed(3);
  }
}

function syncStepInputs(): void {
  for (const id of ["yaw", "pitch", "roll"] as const) {
    document.querySelector<HTMLInputElement>(`#${id}`)!.step = String(state.step);
  }
}

async function openTargetImage(): Promise<void> {
  try {
    const selected = await open({
      title: "Open equirectangular panorama",
      multiple: false,
      defaultPath: state.lastImageDirectory || undefined,
      filters: [
        {
          name: "Images",
          extensions: ["jpg", "jpeg", "png", "tif", "tiff"],
        },
      ],
    });
    if (typeof selected !== "string") return;

    rememberImageDirectory(selected);
    setOpenedImagePath(selected);
    const bytes = await invoke<number[]>("read_file", { path: selected });
    const blob = new Blob([new Uint8Array(bytes)], { type: imageMimeTypeFromPath(selected) });
    await loadTargetFromBlob(selected, blob);
  } catch (error) {
    alert(`Could not open target image: ${String(error)}`);
  }
}

async function addReferenceImages(): Promise<void> {
  try {
    const remainingSlots = MAX_IMAGE_LAYERS - state.layers.length;
    if (remainingSlots <= 0) {
      alert(`PanoPose can show up to ${MAX_IMAGE_LAYERS} image layers at once.`);
      return;
    }

    const selected = await open({
      title: "Add reference panorama",
      multiple: true,
      defaultPath: state.lastImageDirectory || undefined,
      filters: [
        {
          name: "Images",
          extensions: ["jpg", "jpeg", "png", "tif", "tiff"],
        },
      ],
    });
    const paths = selectedToPaths(selected).slice(0, remainingSlots);
    if (paths.length > 0) {
      rememberImageDirectory(paths[0]);
    }
    for (const path of paths) {
      const bytes = await invoke<number[]>("read_file", { path });
      const blob = new Blob([new Uint8Array(bytes)], { type: imageMimeTypeFromPath(path) });
      await addReferenceFromBlob(path, blob);
    }
    if (paths.length > 0) {
      renderLayerList();
      updateLayerRendering();
    }
  } catch (error) {
    alert(`Could not add reference image: ${String(error)}`);
  }
}

async function loadExifMetadataFromImage(): Promise<void> {
  try {
    const selected = await open({
      title: "Load EXIF from image",
      multiple: false,
      defaultPath: state.lastImageDirectory || undefined,
      filters: [
        {
          name: "Images",
          extensions: ["jpg", "jpeg", "png", "tif", "tiff"],
        },
      ],
    });
    if (typeof selected !== "string") return;

    rememberImageDirectory(selected);
    const bytes = await invoke<number[]>("read_file", { path: selected });
    const blob = new Blob([new Uint8Array(bytes)], { type: imageMimeTypeFromPath(selected) });
    const metadata = await readImageMetadata(blob);
    if (!metadata || (!hasTimeMetadata(metadata) && !hasSiteMetadata(metadata))) {
      alert("No usable EXIF time or site metadata found.");
      return;
    }

    const choice = await chooseMetadataImport(metadata, basename(selected));
    if (!choice) return;

    applyImageMetadata(metadata, {
      time: choice === "time" || choice === "both",
      site: choice === "site" || choice === "both",
      pose: false,
    });
    await refreshAstronomy();
    if (choice === "time" || choice === "both") {
      await enablePlanetariumIfSunIsBelowHorizon();
    }
    document.querySelector("#image-status")!.textContent = `Loaded EXIF from ${basename(selected)}`;
  } catch (error) {
    alert(`Could not load EXIF metadata: ${String(error)}`);
  }
}

async function loadTargetFromBlob(path: string, metadataSource: Blob): Promise<void> {
  await applyTargetMetadataFromImage(metadataSource);
  replaceLayer(
    await createImageLayer({
      kind: "target",
      path,
      file: metadataSource,
      pose: state.pose,
      opacity: 1,
      visible: true,
      renderOrder: 1,
    }),
  );
  renderLayerList();
  updateLayerRendering();
  const target = getTargetLayer();
  if (target) {
    document.querySelector("#image-status")!.textContent = `${target.dimensions.width}x${target.dimensions.height}`;
  }
  await applySkyPreview();
  await refreshAstronomy();
}

async function addReferenceFromBlob(path: string, file: Blob): Promise<void> {
  const metadata = await readImageMetadata(file);
  const pose = poseFromMetadata(metadata?.pose, DEFAULT_POSE);
  const layer = await createImageLayer({
    kind: "reference",
    path,
    file,
    pose,
    opacity: 0.5,
    visible: true,
    renderOrder: state.layers.length + 1,
    hasPoseMetadata: metadata?.pose !== null && metadata?.pose !== undefined,
  });
  state.layers.push(layer);
  scene.add(layer.mesh);
  if (!layer.hasPoseMetadata) {
    document.querySelector("#image-status")!.textContent = "Reference loaded without pose metadata";
  }
}

async function createImageLayer(options: {
  kind: LayerKind;
  path: string;
  file: Blob;
  pose: Pose;
  opacity: number;
  visible: boolean;
  renderOrder: number;
  hasPoseMetadata?: boolean;
}): Promise<ImageLayer> {
  const objectUrl = URL.createObjectURL(options.file);
  const texture = await new THREE.TextureLoader().loadAsync(objectUrl);
  texture.colorSpace = THREE.SRGBColorSpace;
  const material = new THREE.MeshBasicMaterial({
    map: texture,
    color: 0xffffff,
    side: THREE.DoubleSide,
    transparent: true,
    opacity: options.opacity,
    depthTest: false,
    depthWrite: false,
  });
  const mesh = new THREE.Mesh(sphereGeometry, material);
  mesh.renderOrder = options.renderOrder;
  const layer: ImageLayer = {
    id: `${options.kind}-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    kind: options.kind,
    name: basename(options.path),
    path: options.path,
    pose: { ...options.pose },
    opacity: options.opacity,
    visible: options.visible,
    objectUrl,
    texture,
    material,
    mesh,
    skyPreviewObjectUrl: null,
    skyPreviewTexture: null,
    skyPreviewCacheKey: null,
    dimensions: {
      width: texture.image.width,
      height: texture.image.height,
    },
    hasPoseMetadata: options.hasPoseMetadata ?? true,
  };
  applyLayerPose(layer);
  return layer;
}

function replaceLayer(layer: ImageLayer): void {
  const existingIndex = state.layers.findIndex((candidate) => candidate.kind === layer.kind);
  if (existingIndex >= 0) {
    disposeImageLayer(state.layers[existingIndex]);
    state.layers.splice(existingIndex, 1, layer);
  } else {
    state.layers.unshift(layer);
  }
  scene.add(layer.mesh);
  updateLayerRenderOrders();
}

function disposeImageLayer(layer: ImageLayer): void {
  scene.remove(layer.mesh);
  disposeSkyPreview(layer);
  layer.texture.dispose();
  layer.material.dispose();
  URL.revokeObjectURL(layer.objectUrl);
}

function applyLayerPose(layer: ImageLayer): void {
  layer.mesh.rotation.set(
    THREE.MathUtils.degToRad(layer.pose.pitch),
    THREE.MathUtils.degToRad(PANORAMA_BASE_YAW_DEG + layer.pose.yaw),
    THREE.MathUtils.degToRad(layer.pose.roll),
    "YXZ",
  );
}

function poseFromMetadata(metadataPose: Partial<Pose> | null | undefined, fallback: Pose): Pose {
  return {
    yaw: metadataPose?.yaw ?? fallback.yaw,
    pitch: metadataPose?.pitch ?? fallback.pitch,
    roll: metadataPose?.roll ?? fallback.roll,
  };
}

function selectedToPaths(selected: string | string[] | null): string[] {
  if (Array.isArray(selected)) return selected.filter((path) => typeof path === "string");
  if (typeof selected === "string") return [selected];
  return [];
}

function basename(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function rememberImageDirectory(path: string): void {
  const directory = dirname(path);
  if (directory) {
    state.lastImageDirectory = directory;
  }
}

function dirname(path: string): string {
  const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return index > 0 ? path.slice(0, index) : "";
}

function imageMimeTypeFromPath(path: string): string {
  const extension = path.split(".").pop()?.toLowerCase();
  if (extension === "jpg" || extension === "jpeg") return "image/jpeg";
  if (extension === "png") return "image/png";
  if (extension === "tif" || extension === "tiff") return "image/tiff";
  return "application/octet-stream";
}

function navigationDragScale(): { horizontalRadiansPerPixel: number; verticalRadiansPerPixel: number } {
  const rect = canvas.getBoundingClientRect();
  const verticalFov = THREE.MathUtils.degToRad(camera.fov);
  const horizontalFov = 2 * Math.atan(Math.tan(verticalFov / 2) * camera.aspect);
  return {
    horizontalRadiansPerPixel: horizontalFov / Math.max(rect.width, 1),
    verticalRadiansPerPixel: verticalFov / Math.max(rect.height, 1),
  };
}

async function refreshAstronomy(): Promise<void> {
  const latitude = Number(document.querySelector<HTMLInputElement>("#latitude")!.value);
  const longitude = Number(document.querySelector<HTMLInputElement>("#longitude")!.value);
  const elevation = Number(document.querySelector<HTMLInputElement>("#elevation")!.value);
  const localTime = document.querySelector<HTMLInputElement>("#time")!.value;
  const timezone = document.querySelector<HTMLInputElement>("#timezone")!.value.trim() || systemTimeZone;
  const objects: CelestialObject[] = ["sun", "moon", "venus", "mars", "jupiter", "saturn", "vega", "sirius"];

  try {
    const isoTime = zonedLocalDateTimeToIso(localTime, timezone);
    state.markers = await invoke<CelestialMarker[]>("astronomy_markers", {
      request: {
        latitude_deg: latitude,
        longitude_deg: longitude,
        elevation_m: elevation,
        time: isoTime,
        objects,
      },
    });
    state.starMarkers = state.planetariumMode
      ? await invoke<StarMarker[]>("star_markers", {
          request: {
            latitude_deg: latitude,
            longitude_deg: longitude,
            elevation_m: elevation,
            time: isoTime,
          },
        })
      : [];
  } catch {
    state.markers = approximateBrowserMarkers(objects);
    state.starMarkers = [];
  }
  drawMarkers();
  drawStars();
  updateReadout();
}

async function enablePlanetariumIfSunIsBelowHorizon(): Promise<void> {
  const sun = state.markers.find((marker) => marker.object === "sun");
  if (!sun || sun.alt_az.altitude_deg >= 0 || state.planetariumMode) return;

  state.planetariumMode = true;
  document.querySelector<HTMLInputElement>("#planetarium-mode")!.checked = true;
  await refreshAstronomy();
}

function zonedLocalDateTimeToIso(value: string, timeZone: string): string {
  const parts = parseLocalDateTime(value);
  const explicitOffset = parseUtcOffsetMinutes(timeZone);
  if (explicitOffset !== null) {
    return `${pad(parts.year, 4)}-${pad(parts.month, 2)}-${pad(parts.day, 2)}T${pad(parts.hour, 2)}:${pad(
      parts.minute,
      2,
    )}:${pad(parts.second, 2)}${formatOffset(explicitOffset)}`;
  }
  const utcGuess = Date.UTC(parts.year, parts.month - 1, parts.day, parts.hour, parts.minute, parts.second);
  const offsetMinutes = getTimeZoneOffsetMinutes(new Date(utcGuess), timeZone, parts);
  return `${pad(parts.year, 4)}-${pad(parts.month, 2)}-${pad(parts.day, 2)}T${pad(parts.hour, 2)}:${pad(
    parts.minute,
    2,
  )}:${pad(parts.second, 2)}${formatOffset(offsetMinutes)}`;
}

function getTargetLayer(): ImageLayer | null {
  return state.layers.find((layer) => layer.kind === "target") ?? null;
}

function getReferenceLayers(): ImageLayer[] {
  return state.layers.filter((layer) => layer.kind === "reference");
}

function setComparisonMode(mode: ComparisonMode): void {
  if ((mode === "reference" || mode === "blink") && getReferenceLayers().length === 0) {
    document.querySelector("#image-status")!.textContent = "No reference layer";
    mode = getTargetLayer() ? "target" : "blend";
  }
  state.comparisonMode = mode;
  state.blinkShowTarget = true;
  for (const comparisonMode of ["blend", "target", "reference", "blink"] as const) {
    document.querySelector(`#compare-${comparisonMode}`)!.classList.toggle("active", comparisonMode === mode);
  }
  updateLayerRendering();
  updateReadout();
}

function updateLayerRendering(renderRows = true): void {
  stopBlinkTimer();
  applyComparisonVisibility(renderRows);
  if (state.comparisonMode === "blink" && getTargetLayer() && getReferenceLayers().length > 0) {
    startBlinkTimer();
  }
}

function startBlinkTimer(): void {
  blinkTimer = window.setInterval(() => {
    state.blinkShowTarget = !state.blinkShowTarget;
    applyComparisonVisibility(false);
    updateReadout();
  }, BLINK_INTERVAL_MS);
}

function stopBlinkTimer(): void {
  if (blinkTimer !== null) {
    window.clearInterval(blinkTimer);
    blinkTimer = null;
  }
}

function applyComparisonVisibility(renderRows = true): void {
  const target = getTargetLayer();
  const references = getReferenceLayers();
  const soloReference = references.find((layer) => layer.visible) ?? references[0] ?? null;

  placeholderPanorama.visible = state.layers.length === 0;

  for (const layer of state.layers) {
    let visible = layer.visible;
    if (state.comparisonMode === "target") {
      visible = layer.kind === "target";
    } else if (state.comparisonMode === "reference") {
      visible = layer.id === soloReference?.id;
    } else if (state.comparisonMode === "blink") {
      visible = state.blinkShowTarget ? layer.id === target?.id : layer.id === soloReference?.id;
    }

    layer.mesh.visible = visible;
    layer.material.opacity = state.comparisonMode === "blend" ? layer.opacity : 1;
    layer.material.needsUpdate = true;
  }

  if (renderRows) {
    renderLayerList();
  }
}

function updateLayerRenderOrders(): void {
  state.layers.forEach((layer, index) => {
    layer.mesh.renderOrder = index + 1;
  });
}

function renderLayerList(): void {
  const layerList = document.querySelector<HTMLDivElement>("#layer-list")!;
  if (state.layers.length === 0) {
    layerList.innerHTML = `<div class="empty-layers">Open a target image or add a reference.</div>`;
    return;
  }

  layerList.innerHTML = state.layers
    .map((layer) => {
      const active = layer.mesh.visible;
      const label = layer.kind === "target" ? "Target" : layer.hasPoseMetadata ? "Reference" : "Reference, no pose";
      return `
        <div class="layer-row ${active ? "active" : ""}" data-layer-id="${layer.id}">
          <div class="layer-main">
            <span class="layer-kind">${label}</span>
            <span class="layer-name" title="${escapeHtml(layer.path)}">${escapeHtml(layer.name)}</span>
          </div>
          <label class="layer-toggle">
            <input type="checkbox" data-layer-action="visible" ${layer.visible ? "checked" : ""} />
            Show
          </label>
          <label class="layer-opacity">
            <span>${Math.round(layer.opacity * 100)}%</span>
            <input type="range" min="0" max="1" step="0.01" value="${layer.opacity}" data-layer-action="opacity" />
          </label>
          ${
            layer.kind === "reference"
              ? `<button class="layer-remove" type="button" data-layer-action="remove">Remove</button>`
              : ""
          }
        </div>
      `;
    })
    .join("");
}

function handleLayerListInput(event: Event): void {
  const input = event.target as HTMLInputElement;
  const action = input.dataset.layerAction;
  const layer = findLayerFromEventTarget(input);
  if (!action || !layer) return;

  if (action === "visible") {
    layer.visible = input.checked;
    updateLayerRendering();
  } else if (action === "opacity") {
    layer.opacity = Number(input.value);
    const valueLabel = input.parentElement?.querySelector("span");
    if (valueLabel) {
      valueLabel.textContent = `${Math.round(layer.opacity * 100)}%`;
    }
    updateLayerRendering(false);
  }
}

function handleLayerListClick(event: Event): void {
  const target = event.target as HTMLElement;
  const action = target.dataset.layerAction;
  const layer = findLayerFromEventTarget(target);
  if (action !== "remove" || !layer || layer.kind === "target") return;

  disposeImageLayer(layer);
  state.layers = state.layers.filter((candidate) => candidate.id !== layer.id);
  updateLayerRenderOrders();
  updateLayerRendering();
}

function findLayerFromEventTarget(target: HTMLElement): ImageLayer | null {
  const row = target.closest<HTMLElement>(".layer-row");
  if (!row) return null;
  const id = row.dataset.layerId;
  return state.layers.find((layer) => layer.id === id) ?? null;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    const replacements: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#039;",
    };
    return replacements[character];
  });
}

async function applyTargetMetadataFromImage(file: Blob): Promise<void> {
  const metadata = await readImageMetadata(file);
  if (!metadata) {
    setCaptureTimeFromReference("No target EXIF time");
    return;
  }

  applyImageMetadata(metadata, { time: true, site: true, pose: true });
  setCaptureTimeFromMetadata(metadata, "Target EXIF");
}

function applyImageMetadata(
  metadata: ImageMetadataResult,
  options: { time: boolean; site: boolean; pose: boolean },
): void {
  if (options.time && metadata.timestamp) {
    document.querySelector<HTMLInputElement>("#time")!.value = metadata.timestamp.localDateTime;
    if (metadata.timestamp.offset) {
      document.querySelector<HTMLInputElement>("#timezone")!.value = metadata.timestamp.offset;
    }
  }
  if (options.time && metadata.timezone) {
    document.querySelector<HTMLInputElement>("#timezone")!.value = metadata.timezone;
  }
  if (options.site && metadata.latitude !== null) {
    document.querySelector<HTMLInputElement>("#latitude")!.value = String(metadata.latitude);
  }
  if (options.site && metadata.longitude !== null) {
    document.querySelector<HTMLInputElement>("#longitude")!.value = String(metadata.longitude);
  }
  if (options.site && metadata.elevation !== null) {
    document.querySelector<HTMLInputElement>("#elevation")!.value = String(metadata.elevation);
  }
  if (options.pose && metadata.pose) {
    state.pose.yaw = metadata.pose.yaw ?? state.pose.yaw;
    state.pose.pitch = metadata.pose.pitch ?? state.pose.pitch;
    state.pose.roll = metadata.pose.roll ?? state.pose.roll;
    syncPoseInputs();
    applyPose();
  }
  updateReadout();
}

function setCaptureTimeFromMetadata(metadata: ImageMetadataResult, source: string): void {
  if (metadata.timestamp) {
    const timezone = metadata.timezone ?? metadata.timestamp.offset ?? systemTimeZone;
    setCaptureTime(metadata.timestamp.localDateTime, timezone, source);
    return;
  }

  setCaptureTimeFromReference("No target EXIF time");
}

function setCaptureTimeFromReference(source: string): void {
  const referenceTime = document.querySelector<HTMLInputElement>("#time")!.value;
  const referenceTimezone = document.querySelector<HTMLInputElement>("#timezone")!.value.trim() || systemTimeZone;
  setCaptureTime(referenceTime, referenceTimezone, source);
}

function useReferenceTimeForCapture(): void {
  setCaptureTimeFromReference("Reference time");
}

function setCaptureTime(localDateTime: string, timezone: string, source: string): void {
  state.captureTime = {
    localDateTime,
    timezone: timezone.trim() || systemTimeZone,
    source,
  };
  syncCaptureTimeInputs();
}

function syncCaptureTimeStateFromInputs(source: string): void {
  const localDateTime = document.querySelector<HTMLInputElement>("#capture-time")!.value;
  const timezone = document.querySelector<HTMLInputElement>("#capture-timezone")!.value.trim() || systemTimeZone;
  state.captureTime = { localDateTime, timezone, source };
  updateCaptureTimeSource();
}

function syncCaptureTimeInputs(): void {
  document.querySelector<HTMLInputElement>("#capture-time")!.value = state.captureTime.localDateTime;
  document.querySelector<HTMLInputElement>("#capture-timezone")!.value = state.captureTime.timezone;
  updateCaptureTimeSource();
}

function updateCaptureTimeSource(): void {
  document.querySelector("#capture-time-source")!.textContent = `Capture time: ${state.captureTime.source}`;
}

function hasTimeMetadata(metadata: ImageMetadataResult): boolean {
  return metadata.timestamp !== null || metadata.timezone !== null;
}

function hasSiteMetadata(metadata: ImageMetadataResult): boolean {
  return metadata.latitude !== null || metadata.longitude !== null || metadata.elevation !== null;
}

function chooseMetadataImport(
  metadata: ImageMetadataResult,
  filename: string,
): Promise<MetadataImportChoice | null> {
  const modal = document.querySelector<HTMLDivElement>("#metadata-import-modal")!;
  const summary = document.querySelector<HTMLDivElement>("#metadata-import-summary")!;
  const timeButton = document.querySelector<HTMLButtonElement>("#metadata-import-time")!;
  const siteButton = document.querySelector<HTMLButtonElement>("#metadata-import-site")!;
  const bothButton = document.querySelector<HTMLButtonElement>("#metadata-import-both")!;
  const cancelButton = document.querySelector<HTMLButtonElement>("#metadata-import-cancel")!;
  const hasTime = hasTimeMetadata(metadata);
  const hasSite = hasSiteMetadata(metadata);

  summary.innerHTML = metadataSummaryHtml(metadata, filename);
  timeButton.disabled = !hasTime;
  siteButton.disabled = !hasSite;
  bothButton.disabled = !hasTime || !hasSite;
  modal.hidden = false;

  return new Promise((resolve) => {
    const cleanup = () => {
      modal.hidden = true;
      timeButton.removeEventListener("click", importTime);
      siteButton.removeEventListener("click", importSite);
      bothButton.removeEventListener("click", importBoth);
      cancelButton.removeEventListener("click", cancel);
      modal.removeEventListener("click", backdropCancel);
      document.removeEventListener("keydown", escapeCancel);
    };
    const finish = (choice: MetadataImportChoice | null) => {
      cleanup();
      resolve(choice);
    };
    const importTime = () => finish("time");
    const importSite = () => finish("site");
    const importBoth = () => finish("both");
    const cancel = () => finish(null);
    const backdropCancel = (event: MouseEvent) => {
      if (event.target === modal) finish(null);
    };
    const escapeCancel = (event: KeyboardEvent) => {
      if (event.key === "Escape") finish(null);
    };

    timeButton.addEventListener("click", importTime);
    siteButton.addEventListener("click", importSite);
    bothButton.addEventListener("click", importBoth);
    cancelButton.addEventListener("click", cancel);
    modal.addEventListener("click", backdropCancel);
    document.addEventListener("keydown", escapeCancel);
  });
}

function metadataSummaryHtml(metadata: ImageMetadataResult, filename: string): string {
  const timeValue = metadata.timestamp
    ? `${metadata.timestamp.localDateTime}${metadata.timestamp.offset ? ` ${metadata.timestamp.offset}` : ""}`
    : "Not found";
  const timezoneValue = metadata.timezone ?? metadata.timestamp?.offset ?? "Not found";
  const latitudeValue = metadata.latitude === null ? "Not found" : formatCoordinate(metadata.latitude);
  const longitudeValue = metadata.longitude === null ? "Not found" : formatCoordinate(metadata.longitude);
  const elevationValue = metadata.elevation === null ? "Not found" : `${metadata.elevation.toFixed(1)} m`;

  return `
    <div class="metadata-file" title="${escapeHtml(filename)}">${escapeHtml(filename)}</div>
    <dl>
      <dt>Time</dt>
      <dd>${escapeHtml(timeValue)}</dd>
      <dt>Time zone</dt>
      <dd>${escapeHtml(timezoneValue)}</dd>
      <dt>Latitude</dt>
      <dd>${escapeHtml(latitudeValue)}</dd>
      <dt>Longitude</dt>
      <dd>${escapeHtml(longitudeValue)}</dd>
      <dt>Elevation</dt>
      <dd>${escapeHtml(elevationValue)}</dd>
    </dl>
  `;
}

function formatCoordinate(value: number): string {
  return value.toFixed(6);
}

async function readImageMetadata(file: Blob): Promise<ImageMetadataResult | null> {
  try {
    const tags = await exifr.parse(file, {
      xmp: true,
      exif: true,
      gps: true,
      reviveValues: false,
    });
    if (!tags) return null;

    const rawDate =
      tags.DateTimeOriginal ?? tags.CreateDate ?? tags.DateCreated ?? tags.ModifyDate ?? tags.DateTime ?? null;
    const localDateTime = normalizeExifDateTime(rawDate);
    const rawOffset = tags.OffsetTimeOriginal ?? tags.OffsetTimeDigitized ?? tags.OffsetTime ?? null;
    const offset = normalizeExifOffset(rawOffset);
    const panopose = parsePanoposeDescription(tags.Description ?? tags.ImageDescription);
    const gpano = parseGpanoPose(tags);
    const panoposeCaptureTime = normalizeExifDateTime(panopose?.capture_time);
    const panoposeCaptureOffset = normalizeIsoOffset(panopose?.capture_time);
    const timestamp = localDateTime
      ? { localDateTime, offset }
      : panoposeCaptureTime
        ? { localDateTime: panoposeCaptureTime, offset: panoposeCaptureOffset }
        : null;

    return {
      timestamp,
      latitude: normalizeNumber(tags.GPSLatitude) ?? panopose?.latitude_deg ?? null,
      longitude: normalizeNumber(tags.GPSLongitude) ?? panopose?.longitude_deg ?? null,
      elevation: normalizeGpsAltitude(tags.GPSAltitude, tags.GPSAltitudeRef) ?? panopose?.elevation_m ?? null,
      pose:
        gpano === null && panopose === null
          ? null
          : {
              yaw: gpano?.heading_deg ?? panopose?.yaw_deg ?? undefined,
              pitch: gpano?.roll_deg ?? panopose?.roll_deg ?? undefined,
              roll: gpano?.pitch_deg ?? panopose?.pitch_deg ?? undefined,
            },
      timezone: panopose?.timezone ?? null,
    };
  } catch {
    return null;
  }
}

function parseGpanoPose(tags: Record<string, unknown>): GpanoPoseMetadata | null {
  const heading = normalizeNumber(tags.PoseHeadingDegrees);
  const pitch = normalizeNumber(tags.PosePitchDegrees);
  const roll = normalizeNumber(tags.PoseRollDegrees);
  if (heading === null && pitch === null && roll === null) return null;
  return {
    heading_deg: heading,
    pitch_deg: pitch,
    roll_deg: roll,
  };
}

function parsePanoposeDescription(value: unknown): PanoposeMetadata | null {
  if (typeof value !== "string") return null;
  try {
    const parsed = JSON.parse(value) as { panopose?: Record<string, number | string> };
    const metadata = parsed.panopose;
    if (!metadata) return null;
    return {
      yaw_deg: normalizeNumber(metadata.yaw_deg),
      pitch_deg: normalizeNumber(metadata.pitch_deg),
      roll_deg: normalizeNumber(metadata.roll_deg),
      latitude_deg: normalizeNumber(metadata.latitude_deg),
      longitude_deg: normalizeNumber(metadata.longitude_deg),
      elevation_m: normalizeNumber(metadata.elevation_m),
      capture_time: typeof metadata.capture_time === "string" ? metadata.capture_time : null,
      reference_time: typeof metadata.reference_time === "string" ? metadata.reference_time : null,
      timezone: typeof metadata.timezone === "string" ? metadata.timezone : null,
    };
  } catch {
    return null;
  }
}

function normalizeNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return null;
}

function normalizeGpsAltitude(value: unknown, ref: unknown): number | null {
  const altitude = normalizeNumber(value);
  if (altitude === null) return null;
  const refNumber = normalizeNumber(ref);
  return refNumber === 1 ? -altitude : altitude;
}

function setOpenedImagePath(path: string): void {
  state.openedImagePath = path.trim();
  state.savePath = state.openedImagePath;
  document.querySelector<HTMLInputElement>("#save-path")!.value = state.savePath;
  updateReadout();
}

function currentSkyRemovalSettings(): SkyRemovalSettings {
  return {
    sensitivity: state.skyRemoval.sensitivity,
  };
}

function currentSkyPreviewCacheKey(target: ImageLayer): SkyPreviewCacheKey {
  return {
    path: target.path,
    sensitivity: state.skyRemoval.sensitivity,
    pose: { ...state.pose },
  };
}

function skyPreviewCacheMatches(layer: ImageLayer, cacheKey: SkyPreviewCacheKey): boolean {
  const cached = layer.skyPreviewCacheKey;
  return (
    cached !== null &&
    cached.path === cacheKey.path &&
    cached.sensitivity === cacheKey.sensitivity &&
    cached.pose.yaw === cacheKey.pose.yaw &&
    cached.pose.pitch === cacheKey.pose.pitch &&
    cached.pose.roll === cacheKey.pose.roll
  );
}

function scheduleSkyPreview(): void {
  if (skyPreviewTimer !== null) {
    window.clearTimeout(skyPreviewTimer);
  }
  skyPreviewTimer = window.setTimeout(() => {
    skyPreviewTimer = null;
    void applySkyPreview();
  }, 250);
}

async function applySkyPreview(): Promise<void> {
  const target = getTargetLayer();
  state.skyRemoval.previewRequestId += 1;
  const requestId = state.skyRemoval.previewRequestId;
  if (!target) return;

  if (!state.skyRemoval.enabled) {
    setTargetTexture(target, target.texture);
    document.querySelector("#image-status")!.textContent = `${target.dimensions.width}x${target.dimensions.height}`;
    return;
  }

  const cacheKey = currentSkyPreviewCacheKey(target);
  if (target.skyPreviewTexture && skyPreviewCacheMatches(target, cacheKey)) {
    setTargetTexture(target, target.skyPreviewTexture);
    document.querySelector("#image-status")!.textContent = "Sky preview enabled";
    return;
  }

  document.querySelector("#image-status")!.textContent = "Detecting sky";
  try {
    const bytes = await invoke<number[]>("preview_sky_removed_image", {
      request: {
        input: target.path,
        settings: currentSkyRemovalSettings(),
        max_width: 4096,
      },
    });
    if (requestId !== state.skyRemoval.previewRequestId || !state.skyRemoval.enabled) return;

    const blob = new Blob([new Uint8Array(bytes)], { type: "image/png" });
    const objectUrl = URL.createObjectURL(blob);
    const texture = await new THREE.TextureLoader().loadAsync(objectUrl);
    texture.colorSpace = THREE.SRGBColorSpace;
    if (requestId !== state.skyRemoval.previewRequestId || !state.skyRemoval.enabled) {
      texture.dispose();
      URL.revokeObjectURL(objectUrl);
      return;
    }

    disposeSkyPreview(target);
    target.skyPreviewObjectUrl = objectUrl;
    target.skyPreviewTexture = texture;
    target.skyPreviewCacheKey = cacheKey;
    setTargetTexture(target, texture);
    document.querySelector("#image-status")!.textContent = "Sky preview enabled";
  } catch (error) {
    setTargetTexture(target, target.texture);
    document.querySelector<HTMLInputElement>("#remove-sky")!.checked = false;
    state.skyRemoval.enabled = false;
    alert(`Could not detect sky: ${String(error)}`);
  }
}

function setTargetTexture(layer: ImageLayer, texture: THREE.Texture): void {
  layer.material.map = texture;
  layer.material.needsUpdate = true;
}

function disposeSkyPreview(layer: ImageLayer): void {
  if (layer.skyPreviewTexture) {
    layer.skyPreviewTexture.dispose();
    layer.skyPreviewTexture = null;
  }
  if (layer.skyPreviewObjectUrl) {
    URL.revokeObjectURL(layer.skyPreviewObjectUrl);
    layer.skyPreviewObjectUrl = null;
  }
  layer.skyPreviewCacheKey = null;
}

async function exportCurrentTargetImage(): Promise<void> {
  try {
    const target = getTargetLayer();
    if (!target || !state.openedImagePath) {
      alert("Open a target image before exporting.");
      return;
    }

    const selected = await save({
      title: "Export oriented panorama",
      defaultPath: state.lastImageDirectory || undefined,
      filters: [
        {
          name: "PNG",
          extensions: ["png"],
        },
      ],
    });
    if (!selected) return;

    const outputPath = pngExportPath(selected);
    const existingFile = await invoke<boolean>("path_exists", { path: outputPath }).catch(() => false);
    const overwriteExisting = existingFile ? await confirmExportOverwrite(outputPath) : true;
    if (!overwriteExisting) return;

    let unlistenProgress: UnlistenFn | null = await listen<ExportProgress>("export-progress", (event) => {
      updateExportProgress(event.payload.completed_rows, event.payload.total_rows);
    });

    showExportProgress(outputPath, target.dimensions.width, target.dimensions.height);
    await waitForNextPaint();
    try {
      await invoke("export_image", {
        request: {
          input: state.openedImagePath,
          output: outputPath,
          width: target.dimensions.width,
          height: target.dimensions.height,
          center_azimuth_deg: 180.0,
          yaw_deg: state.pose.yaw,
          pitch_deg: state.pose.pitch,
          roll_deg: state.pose.roll,
          sky_removal: state.skyRemoval.enabled ? currentSkyRemovalSettings() : null,
        },
      });
    } finally {
      unlistenProgress?.();
      unlistenProgress = null;
      hideExportProgress();
    }
    rememberImageDirectory(outputPath);
    document.querySelector("#image-status")!.textContent =
      `Exported ${target.dimensions.width}x${target.dimensions.height} PNG`;
  } catch (error) {
    alert(`Could not export image: ${String(error)}`);
  }
}

async function exportCurrentTargetStellariumZip(): Promise<void> {
  try {
    const target = getTargetLayer();
    if (!target || !state.openedImagePath) {
      alert("Open a target image before exporting a Stellarium landscape.");
      return;
    }

    const details = await chooseStellariumExportDetails(target);
    if (!details) return;

    const existingFile = await invoke<boolean>("path_exists", { path: details.outputZip }).catch(() => false);
    const overwriteExisting = existingFile ? await confirmStellariumOverwrite(details.outputZip) : true;
    if (!overwriteExisting) return;

    const latitude = Number(document.querySelector<HTMLInputElement>("#latitude")!.value);
    const longitude = Number(document.querySelector<HTMLInputElement>("#longitude")!.value);
    const elevation = Number(document.querySelector<HTMLInputElement>("#elevation")!.value);

    let unlistenProgress: UnlistenFn | null = await listen<ExportProgress>("export-progress", (event) => {
      updateExportProgress(event.payload.completed_rows, event.payload.total_rows);
    });

    showExportProgress(details.outputZip, target.dimensions.width, target.dimensions.height, "Stellarium landscape ZIP");
    await waitForNextPaint();
    try {
      await invoke("export_stellarium_landscape", {
        request: {
          input: state.openedImagePath,
          output_zip: details.outputZip,
          directory_name: details.directoryName,
          texture_filename: details.textureFilename,
          landscape_name: details.landscapeName,
          author: details.author,
          description: details.description,
          width: target.dimensions.width,
          height: target.dimensions.height,
          yaw_deg: state.pose.yaw,
          pitch_deg: state.pose.pitch,
          roll_deg: state.pose.roll,
          sky_removal: state.skyRemoval.enabled ? currentSkyRemovalSettings() : null,
          latitude_deg: latitude,
          longitude_deg: longitude,
          elevation_m: elevation,
        },
      });
    } finally {
      unlistenProgress?.();
      unlistenProgress = null;
      hideExportProgress();
    }

    rememberImageDirectory(details.outputZip);
    document.querySelector("#image-status")!.textContent = `Exported Stellarium ZIP: ${basename(details.outputZip)}`;
  } catch (error) {
    alert(`Could not export Stellarium landscape: ${String(error)}`);
  }
}

async function chooseStellariumExportDetails(target: ImageLayer): Promise<StellariumExportDetails | null> {
  const sourceName = filenameStem(target.name);
  const defaultDirectory = slugifyFilename(sourceName || "panopose-landscape");
  const defaultTexture = `${defaultDirectory}.png`;
  const outputZip = await save({
    title: "Export Stellarium landscape ZIP",
    defaultPath: state.lastImageDirectory ? `${state.lastImageDirectory}/${defaultDirectory}.zip` : `${defaultDirectory}.zip`,
    filters: [
      {
        name: "ZIP",
        extensions: ["zip"],
      },
    ],
  });
  if (!outputZip) return null;

  return new Promise((resolve) => {
    const modal = document.querySelector<HTMLDivElement>("#stellarium-export-modal")!;
    const nameInput = document.querySelector<HTMLInputElement>("#stellarium-name")!;
    const authorInput = document.querySelector<HTMLInputElement>("#stellarium-author")!;
    const descriptionInput = document.querySelector<HTMLInputElement>("#stellarium-description")!;
    const directoryInput = document.querySelector<HTMLInputElement>("#stellarium-directory")!;
    const textureInput = document.querySelector<HTMLInputElement>("#stellarium-texture")!;
    const exportButton = document.querySelector<HTMLButtonElement>("#stellarium-export-confirm")!;
    const cancelButton = document.querySelector<HTMLButtonElement>("#stellarium-export-cancel")!;

    nameInput.value = sourceName || "PanoPose Landscape";
    authorInput.value = "";
    descriptionInput.value = "";
    directoryInput.value = defaultDirectory;
    textureInput.value = defaultTexture;

    const finish = (details: StellariumExportDetails | null) => {
      modal.hidden = true;
      exportButton.removeEventListener("click", submit);
      cancelButton.removeEventListener("click", cancel);
      modal.removeEventListener("click", backdropCancel);
      document.removeEventListener("keydown", escapeCancel);
      resolve(details);
    };

    const submit = () => {
      const landscapeName = nameInput.value.trim();
      const directoryName = directoryInput.value.trim();
      const textureFilename = pngExportPath(textureInput.value.trim());
      if (!landscapeName || !directoryName || !textureFilename) {
        alert("Landscape name, ZIP directory, and panorama PNG filename are required.");
        return;
      }
      finish({
        outputZip: zipExportPath(outputZip),
        directoryName,
        textureFilename,
        landscapeName,
        author: authorInput.value.trim(),
        description: descriptionInput.value.trim(),
      });
    };

    const cancel = () => finish(null);
    const backdropCancel = (event: MouseEvent) => {
      if (event.target === modal) finish(null);
    };
    const escapeCancel = (event: KeyboardEvent) => {
      if (event.key === "Escape") finish(null);
    };

    exportButton.addEventListener("click", submit);
    cancelButton.addEventListener("click", cancel);
    modal.addEventListener("click", backdropCancel);
    document.addEventListener("keydown", escapeCancel);
    modal.hidden = false;
    nameInput.focus();
  });
}

function pngExportPath(path: string): string {
  const extensionIndex = path.lastIndexOf(".");
  const separatorIndex = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (extensionIndex > separatorIndex && path.slice(extensionIndex).toLowerCase() === ".png") {
    return path;
  }
  return `${path}.png`;
}

function zipExportPath(path: string): string {
  const extensionIndex = path.lastIndexOf(".");
  const separatorIndex = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (extensionIndex > separatorIndex && path.slice(extensionIndex).toLowerCase() === ".zip") {
    return path;
  }
  return `${path}.zip`;
}

function filenameStem(path: string): string {
  const name = basename(path);
  const extensionIndex = name.lastIndexOf(".");
  return extensionIndex > 0 ? name.slice(0, extensionIndex) : name;
}

function slugifyFilename(value: string): string {
  const slug = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "panopose-landscape";
}

async function confirmExportOverwrite(targetPath: string): Promise<boolean> {
  return confirm(
    ["Overwrite the existing PNG export?", "", targetPath, "", "This replaces the existing file."].join("\n"),
    {
      title: "Confirm Export",
      kind: "warning",
      okLabel: "Export",
      cancelLabel: "Cancel",
    },
  );
}

async function confirmStellariumOverwrite(targetPath: string): Promise<boolean> {
  return confirm(
    ["Overwrite the existing Stellarium ZIP export?", "", targetPath, "", "This replaces the existing file."].join("\n"),
    {
      title: "Confirm Stellarium Export",
      kind: "warning",
      okLabel: "Export",
      cancelLabel: "Cancel",
    },
  );
}

function showExportProgress(targetPath: string, width: number, height: number, outputKind = "panorama"): void {
  const modal = document.querySelector<HTMLDivElement>("#export-progress-modal")!;
  const message = document.querySelector<HTMLDivElement>("#export-progress-message")!;
  message.textContent = state.skyRemoval.enabled
    ? `Detecting sky, then exporting ${outputKind} to ${targetPath} at ${width} x ${height}`
    : `Exporting ${outputKind} to ${targetPath} at ${width} x ${height}`;
  updateExportProgress(0, height);
  modal.hidden = false;
}

function updateExportProgress(completedRows: number, totalRows: number): void {
  const total = Math.max(totalRows, 1);
  const completed = Math.min(Math.max(completedRows, 0), total);
  const progressBar = document.querySelector<HTMLProgressElement>("#export-progress-bar")!;
  const percent = document.querySelector<HTMLDivElement>("#export-progress-percent")!;
  progressBar.max = total;
  progressBar.value = completed;
  percent.textContent = `${Math.round((completed / total) * 100)}%`;
}

function hideExportProgress(): void {
  document.querySelector<HTMLDivElement>("#export-progress-modal")!.hidden = true;
}

function waitForNextPaint(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}

async function saveMetadataAs(): Promise<void> {
  try {
    if (!state.openedImagePath) {
      alert("Open an image before using Save As.");
      return;
    }

    const selected = await save({
      title: "Save image metadata as",
      defaultPath: state.savePath || state.openedImagePath,
      filters: [
        {
          name: "Images",
          extensions: ["jpg", "jpeg", "png", "tif", "tiff"],
        },
      ],
    });
    if (!selected) return;

    const existingFile = await invoke<boolean>("path_exists", { path: selected }).catch(() => false);
    const overwriteExisting = existingFile ? await confirmOverwrite(selected) : false;
    if (existingFile && !overwriteExisting) return;
    await writeMetadataToImage(selected, state.openedImagePath, overwriteExisting);
  } catch (error) {
    alert(`Could not save metadata: ${String(error)}`);
  }
}

async function confirmOverwrite(targetPath: string): Promise<boolean> {
  return confirm(
    [
      "Write PanoPose metadata to the existing image file?",
      "",
      targetPath,
      "",
      "This updates the image file in place.",
    ].join("\n"),
    {
      title: "Confirm Metadata Save",
      kind: "warning",
      okLabel: "Save",
      cancelLabel: "Cancel",
    },
  );
}

async function writeMetadataToImage(targetPath: string, sourcePath: string, overwriteExisting: boolean): Promise<void> {
  syncCaptureTimeStateFromInputs(state.captureTime.source);
  const referenceTimezone = document.querySelector<HTMLInputElement>("#timezone")!.value.trim() || systemTimeZone;
  const referenceLocalTime = document.querySelector<HTMLInputElement>("#time")!.value;
  const latitude = Number(document.querySelector<HTMLInputElement>("#latitude")!.value);
  const longitude = Number(document.querySelector<HTMLInputElement>("#longitude")!.value);
  const elevation = Number(document.querySelector<HTMLInputElement>("#elevation")!.value);
  const referenceTime = zonedLocalDateTimeToIso(referenceLocalTime, referenceTimezone);
  const captureTime = zonedLocalDateTimeToIso(state.captureTime.localDateTime, state.captureTime.timezone);

  try {
    await invoke("write_panopose_metadata", {
      request: {
        path: targetPath,
        source_path: sourcePath,
        overwrite_existing: overwriteExisting,
        yaw_deg: state.pose.yaw,
        pitch_deg: state.pose.roll,
        roll_deg: state.pose.pitch,
        latitude_deg: latitude,
        longitude_deg: longitude,
        elevation_m: elevation,
        capture_time: captureTime,
        reference_time: referenceTime,
        timezone: referenceTimezone,
      },
    });
    state.savePath = targetPath;
    document.querySelector<HTMLInputElement>("#save-path")!.value = state.savePath;
    document.querySelector("#image-status")!.textContent = "Metadata written";
  } catch (error) {
    alert(`Could not write metadata: ${String(error)}`);
  }
}

function normalizeExifDateTime(value: unknown): string | null {
  if (value instanceof Date && !Number.isNaN(value.getTime())) {
    return `${pad(value.getFullYear(), 4)}-${pad(value.getMonth() + 1, 2)}-${pad(value.getDate(), 2)}T${pad(
      value.getHours(),
      2,
    )}:${pad(value.getMinutes(), 2)}`;
  }
  if (typeof value !== "string") return null;

  const trimmed = value.trim();
  const exifMatch = /^(\d{4}):(\d{2}):(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?/.exec(trimmed);
  if (exifMatch) {
    return `${exifMatch[1]}-${exifMatch[2]}-${exifMatch[3]}T${exifMatch[4]}:${exifMatch[5]}`;
  }

  const isoMatch = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})/.exec(trimmed);
  if (isoMatch) {
    return `${isoMatch[1]}-${isoMatch[2]}-${isoMatch[3]}T${isoMatch[4]}:${isoMatch[5]}`;
  }

  return null;
}

function normalizeExifOffset(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const match = /^([+-])(\d{2}):?(\d{2})$/.exec(value.trim());
  if (!match) return null;
  return `${match[1]}${match[2]}:${match[3]}`;
}

function normalizeIsoOffset(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const match = /([+-]\d{2}:?\d{2}|Z)$/.exec(value.trim());
  if (!match) return null;
  if (match[1] === "Z") return "+00:00";
  return normalizeExifOffset(match[1]);
}

function parseLocalDateTime(value: string): LocalDateTimeParts {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(value);
  if (!match) {
    throw new Error(`Invalid reference time: ${value}`);
  }
  return {
    year: Number(match[1]),
    month: Number(match[2]),
    day: Number(match[3]),
    hour: Number(match[4]),
    minute: Number(match[5]),
    second: Number(match[6] ?? 0),
  };
}

function getTimeZoneOffsetMinutes(utcGuess: Date, timeZone: string, target: LocalDateTimeParts): number {
  const formatter = new Intl.DateTimeFormat("en-US", {
    timeZone,
    hourCycle: "h23",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  const formatted = formatter.formatToParts(utcGuess);
  const values = new Map(formatted.map((part) => [part.type, Number(part.value)]));
  const zonedAsUtc = Date.UTC(
    values.get("year") ?? target.year,
    (values.get("month") ?? target.month) - 1,
    values.get("day") ?? target.day,
    values.get("hour") ?? target.hour,
    values.get("minute") ?? target.minute,
    values.get("second") ?? target.second,
  );
  const targetAsUtc = Date.UTC(target.year, target.month - 1, target.day, target.hour, target.minute, target.second);
  return Math.round((zonedAsUtc - targetAsUtc) / 60000);
}

function parseUtcOffsetMinutes(value: string): number | null {
  const match = /^UTC?([+-]\d{2}:?\d{2})$|^([+-]\d{2}:?\d{2})$/.exec(value.trim());
  const offset = match?.[1] ?? match?.[2];
  if (!offset) return null;
  const parts = /^([+-])(\d{2}):?(\d{2})$/.exec(offset);
  if (!parts) return null;
  const sign = parts[1] === "+" ? 1 : -1;
  return sign * (Number(parts[2]) * 60 + Number(parts[3]));
}

function formatOffset(offsetMinutes: number): string {
  const sign = offsetMinutes >= 0 ? "+" : "-";
  const abs = Math.abs(offsetMinutes);
  return `${sign}${pad(Math.floor(abs / 60), 2)}:${pad(abs % 60, 2)}`;
}

function pad(value: number, length: number): string {
  return String(value).padStart(length, "0");
}

function approximateBrowserMarkers(objects: CelestialObject[]): CelestialMarker[] {
  return objects.map((object, index) => ({
    object,
    label: object,
    alt_az: { altitude_deg: 15 + index * 4, azimuth_deg: index * 42 },
    magnitude: null,
    accuracy_note: "browser fallback marker",
  }));
}

function updateGridForZoom(): void {
  const spacing = gridSpacingForFov(camera.fov);
  if (spacing === activeGridSpacingDeg) return;
  activeGridSpacingDeg = spacing;
  createGrid(spacing);
}

function gridSpacingForFov(fovDeg: number): number {
  if (fovDeg <= 10) return 1;
  if (fovDeg <= 18) return 2;
  if (fovDeg <= 32) return 5;
  if (fovDeg <= 58) return 10;
  return 30;
}

function createGrid(spacingDeg: number): void {
  disposeGroupChildren(gridGroup);
  const material = new THREE.LineBasicMaterial({ color: 0xd9b44a, transparent: true, opacity: 0.45 });
  const horizonMaterial = new THREE.LineBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.78 });
  const cardinalMaterial = new THREE.LineBasicMaterial({ color: 0x7fd1ff, transparent: true, opacity: 0.72 });
  for (let alt = -90 + spacingDeg; alt < 90; alt += spacingDeg) {
    gridGroup.add(makeAltLine(alt, alt === 0 ? horizonMaterial : material));
  }
  for (let az = 0; az < 360; az += spacingDeg) {
    const compass = compassLabel(az);
    gridGroup.add(makeAzLine(az, compass ? cardinalMaterial : material));
  }
  addGridLabels(spacingDeg);
}

function makeAltLine(altitudeDeg: number, material: THREE.Material): THREE.Line {
  const points: THREE.Vector3[] = [];
  for (let az = 0; az <= 360; az += 2) {
    points.push(altAzToVector(altitudeDeg, az, 499));
  }
  return new THREE.Line(new THREE.BufferGeometry().setFromPoints(points), material);
}

function makeAzLine(azimuthDeg: number, material: THREE.Material): THREE.Line {
  const points: THREE.Vector3[] = [];
  for (let alt = -88; alt <= 88; alt += 2) {
    points.push(altAzToVector(alt, azimuthDeg, 499));
  }
  return new THREE.Line(new THREE.BufferGeometry().setFromPoints(points), material);
}

function addGridLabels(spacingDeg: number): void {
  void spacingDeg;
  for (let az = 0; az < 360; az += 45) {
    const compass = compassLabel(az);
    if (!compass) continue;
    const label = makeTextSprite(`${compass} ${az} deg`, {
      background: "rgba(8, 16, 20, 0.86)",
      border: "rgba(127, 209, 255, 0.95)",
      color: "#e9fbff",
      fontSize: 42,
    });
    label.position.copy(altAzToVector(0, az, 455));
    label.scale.set(64, 24, 1);
    gridGroup.add(label);
  }

  for (const altitude of [-60, -30, 30, 60]) {
    const label = makeTextSprite(`${formatSignedDegree(altitude)} alt`, {
      background: "rgba(10, 12, 14, 0.62)",
      border: "rgba(217, 180, 74, 0.55)",
      color: "#f5d98a",
      fontSize: 30,
    });
    label.position.copy(altAzToVector(altitude, 45, 452));
    label.scale.set(50, 17, 1);
    gridGroup.add(label);
  }
}

function disposeGroupChildren(group: THREE.Group): void {
  while (group.children.length > 0) {
    const child = group.children[0];
    group.remove(child);
    disposeObject(child);
  }
}

function disposeObject(object: THREE.Object3D): void {
  const disposable = object as THREE.Object3D & {
    geometry?: THREE.BufferGeometry;
    material?: THREE.Material | THREE.Material[];
  };

  disposable.geometry?.dispose();
  const materials = Array.isArray(disposable.material)
    ? disposable.material
    : disposable.material
      ? [disposable.material]
      : [];
  for (const material of materials) {
    const maybeTextured = material as THREE.Material & { map?: THREE.Texture };
    maybeTextured.map?.dispose();
    material.dispose();
  }

  for (const child of object.children) {
    disposeObject(child);
  }
}

function compassLabel(azimuthDeg: number): string | null {
  const labels = new Map<number, string>([
    [0, "N"],
    [45, "NE"],
    [90, "E"],
    [135, "SE"],
    [180, "S"],
    [225, "SW"],
    [270, "W"],
    [315, "NW"],
  ]);
  return labels.get(azimuthDeg) ?? null;
}

function formatSignedDegree(value: number): string {
  if (value > 0) return `+${value} deg`;
  return `${value} deg`;
}

function makeTextSprite(
  text: string,
  style: {
    background: string;
    border: string;
    color: string;
    fontSize: number;
  },
): THREE.Sprite {
  const canvas = document.createElement("canvas");
  canvas.width = 512;
  canvas.height = 192;
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("2D canvas context is unavailable");
  }

  context.clearRect(0, 0, canvas.width, canvas.height);
  context.fillStyle = style.background;
  roundRect(context, 18, 34, canvas.width - 36, canvas.height - 68, 24);
  context.fill();
  context.strokeStyle = style.border;
  context.lineWidth = 5;
  context.stroke();

  context.font = `650 ${style.fontSize}px Inter, system-ui, sans-serif`;
  context.textAlign = "center";
  context.textBaseline = "middle";
  context.fillStyle = style.color;
  context.fillText(text, canvas.width / 2, canvas.height / 2 + 1);

  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  const material = new THREE.SpriteMaterial({
    map: texture,
    transparent: true,
    depthTest: false,
    depthWrite: false,
  });
  return new THREE.Sprite(material);
}

function roundRect(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): void {
  context.beginPath();
  context.moveTo(x + radius, y);
  context.lineTo(x + width - radius, y);
  context.quadraticCurveTo(x + width, y, x + width, y + radius);
  context.lineTo(x + width, y + height - radius);
  context.quadraticCurveTo(x + width, y + height, x + width - radius, y + height);
  context.lineTo(x + radius, y + height);
  context.quadraticCurveTo(x, y + height, x, y + height - radius);
  context.lineTo(x, y + radius);
  context.quadraticCurveTo(x, y, x + radius, y);
  context.closePath();
}

function drawMarkers(): void {
  disposeGroupChildren(markerGroup);
  for (const marker of state.markers) {
    const altitude = marker.alt_az.altitude_deg;
    const azimuth = marker.alt_az.azimuth_deg;
    const circle = makeMarkerCircleSprite();
    circle.position.copy(altAzToVector(altitude, azimuth, 470));
    circle.scale.set(28, 28, 1);
    circle.renderOrder = 120;
    markerGroup.add(circle);

    const label = makeTextSprite(marker.label, {
      background: "rgba(6, 18, 24, 0.84)",
      border: "rgba(127, 209, 255, 0.9)",
      color: "#dff7ff",
      fontSize: 38,
    });
    label.position.copy(altAzToVector(labelAltitudeBelowMarker(altitude), azimuth, 456));
    label.scale.set(54, 20, 1);
    label.renderOrder = 121;
    markerGroup.add(label);
  }
}

function drawStars(): void {
  disposeGroupChildren(starGroup);
  if (!state.planetariumMode || state.starMarkers.length === 0) return;

  addStarPointBatch(
    state.starMarkers.filter((star) => star.magnitude <= 1.5),
    5.5,
    0.98,
  );
  addStarPointBatch(
    state.starMarkers.filter((star) => star.magnitude > 1.5 && star.magnitude <= 3.0),
    3.8,
    0.86,
  );
  addStarPointBatch(
    state.starMarkers.filter((star) => star.magnitude > 3.0),
    2.4,
    0.68,
  );
}

function addStarPointBatch(stars: StarMarker[], size: number, opacity: number): void {
  if (stars.length === 0) return;

  const positions = new Float32Array(stars.length * 3);
  stars.forEach((star, index) => {
    const position = altAzToVector(star.alt_az.altitude_deg, star.alt_az.azimuth_deg, 465);
    positions[index * 3] = position.x;
    positions[index * 3 + 1] = position.y;
    positions[index * 3 + 2] = position.z;
  });

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  const material = new THREE.PointsMaterial({
    color: 0xf4f8ff,
    size,
    sizeAttenuation: false,
    transparent: true,
    opacity,
    depthTest: false,
    depthWrite: false,
  });
  const points = new THREE.Points(geometry, material);
  points.renderOrder = 106;
  starGroup.add(points);
}

function makeMarkerCircleSprite(): THREE.Sprite {
  const canvas = document.createElement("canvas");
  canvas.width = 128;
  canvas.height = 128;
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("2D canvas context is unavailable");
  }

  context.clearRect(0, 0, canvas.width, canvas.height);
  context.strokeStyle = "rgba(127, 209, 255, 0.96)";
  context.lineWidth = 10;
  context.beginPath();
  context.arc(64, 64, 45, 0, Math.PI * 2);
  context.stroke();

  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  const material = new THREE.SpriteMaterial({
    map: texture,
    transparent: true,
    depthTest: false,
    depthWrite: false,
  });
  return new THREE.Sprite(material);
}

function labelAltitudeBelowMarker(altitudeDeg: number): number {
  return THREE.MathUtils.clamp(altitudeDeg - 3.2, -88, 88);
}

function altAzToVector(altitudeDeg: number, azimuthDeg: number, radius: number): THREE.Vector3 {
  const alt = THREE.MathUtils.degToRad(altitudeDeg);
  const az = THREE.MathUtils.degToRad(azimuthDeg);
  const r = Math.cos(alt) * radius;
  return new THREE.Vector3(-r * Math.sin(az), Math.sin(alt) * radius, r * Math.cos(az));
}

function vectorToAltAz(vector: THREE.Vector3): AltAzReadout {
  const normalized = vector.clone().normalize();
  return {
    altitudeDeg: THREE.MathUtils.radToDeg(Math.asin(THREE.MathUtils.clamp(normalized.y, -1, 1))),
    azimuthDeg: normalizeDegrees(THREE.MathUtils.radToDeg(Math.atan2(-normalized.x, normalized.z))),
  };
}

function updateCursorAltAz(event: PointerEvent): void {
  const rect = canvas.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) {
    cursorAltAz = null;
    updateReadout();
    return;
  }

  cursorNdc.set(
    ((event.clientX - rect.left) / rect.width) * 2 - 1,
    -((event.clientY - rect.top) / rect.height) * 2 + 1,
  );
  updateCameraAim();
  camera.updateMatrixWorld(true);
  cursorRaycaster.setFromCamera(cursorNdc, camera);
  cursorAltAz = vectorToAltAz(cursorRaycaster.ray.direction);
  updateReadout();
}

function updateReadout(): void {
  const readout = document.querySelector("#readout")!;
  const referenceCount = getReferenceLayers().length;
  const layerMode =
    state.comparisonMode === "blink"
      ? `blink ${state.blinkShowTarget ? "target" : "reference"}`
      : state.comparisonMode;
  const cursorText = cursorAltAz
    ? `cursor alt ${formatSignedFixedDegree(cursorAltAz.altitudeDeg)} az ${formatFixedDegree(cursorAltAz.azimuthDeg)}`
    : "cursor alt -- az --";
  readout.textContent =
    `${state.mode === "navigate" ? "Navigate" : "Align Target"} | ` +
    `${layerMode} | ` +
    `${cursorText} | ` +
    `az ${state.pose.yaw.toFixed(3)} pitch ${state.pose.roll.toFixed(3)} roll ${state.pose.pitch.toFixed(3)} | ` +
    `${referenceCount} reference layer${referenceCount === 1 ? "" : "s"} | ` +
    `${state.markers.length} astronomy markers | ` +
    `${state.planetariumMode ? `${state.starMarkers.length} stars | ` : ""}` +
    `${document.querySelector<HTMLInputElement>("#timezone")?.value || systemTimeZone}`;
}

function formatFixedDegree(value: number): string {
  return `${value.toFixed(2)} deg`;
}

function formatSignedFixedDegree(value: number): string {
  return `${value >= 0 ? "+" : ""}${formatFixedDegree(value)}`;
}

function normalizeDegrees(value: number): number {
  return ((value % 360) + 360) % 360;
}

function resize(): void {
  const rect = canvas.parentElement!.getBoundingClientRect();
  renderer.setSize(rect.width, rect.height, false);
  camera.aspect = rect.width / rect.height;
  camera.updateProjectionMatrix();
}

function animate(): void {
  requestAnimationFrame(animate);
  updateCameraAim();
  renderer.render(scene, camera);
}

function updateCameraAim(): void {
  camera.lookAt(
    Math.sin(yawView) * Math.cos(pitchView),
    Math.sin(pitchView),
    Math.cos(yawView) * Math.cos(pitchView),
  );
}
