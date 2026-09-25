import * as THREE from "three";

export interface SerializedQuaternion {
  w: number;
  x: number;
  y: number;
  z: number;
}

export interface OrientationControls {
  azimuthOffsetDeg: number;
  panoramaUpTiltDeg: number;
  panoramaUpAzimuthDeg: number;
}

export interface AlignmentDragState {
  initialWorldDirection: THREE.Vector3;
  initialMeshQuaternion: THREE.Quaternion;
}

export interface RollDragState {
  axis: THREE.Vector3;
  initialOrientation: THREE.Quaternion;
}

export interface ViewportProjection {
  leftPx: number;
  topPx: number;
}

const WORLD_UP = new THREE.Vector3(0, 1, 0);
const MAX_PANORAMA_UP_TILT_DEG = 180;
const EPSILON = 1e-10;

export const IDENTITY_ORIENTATION: SerializedQuaternion = { w: 1, x: 0, y: 0, z: 0 };

export function orientationFromControls(controls: OrientationControls): SerializedQuaternion {
  const tilt = THREE.MathUtils.degToRad(
    THREE.MathUtils.clamp(controls.panoramaUpTiltDeg, 0, MAX_PANORAMA_UP_TILT_DEG),
  );
  const swing = panoramaUpSwing(tilt, controls.panoramaUpAzimuthDeg);
  const twist = new THREE.Quaternion().setFromAxisAngle(
    WORLD_UP,
    THREE.MathUtils.degToRad(controls.azimuthOffsetDeg),
  );
  return serializeQuaternion(swing.multiply(twist));
}

export function controlsFromOrientation(
  orientation: SerializedQuaternion,
  retainedPanoramaUpAzimuthDeg: number,
): OrientationControls {
  const quaternion = deserializeQuaternion(orientation);
  const panoramaUp = WORLD_UP.clone().applyQuaternion(quaternion).normalize();
  const panoramaUpTiltDeg = THREE.MathUtils.radToDeg(
    Math.acos(THREE.MathUtils.clamp(panoramaUp.y, -1, 1)),
  );
  const horizontalLength = Math.hypot(panoramaUp.x, panoramaUp.z);
  const panoramaUpAzimuthDeg =
    horizontalLength <= EPSILON
      ? normalizeDegrees(retainedPanoramaUpAzimuthDeg)
      : normalizeDegrees(
          THREE.MathUtils.radToDeg(Math.atan2(panoramaUp.x, panoramaUp.z)),
        );
  const swing = panoramaUpSwing(
    THREE.MathUtils.degToRad(panoramaUpTiltDeg),
    panoramaUpAzimuthDeg,
  );
  const twist = swing.invert().multiply(quaternion).normalize();
  const azimuthOffsetDeg = normalizeDegrees(
    THREE.MathUtils.radToDeg(2 * Math.atan2(twist.y, twist.w)),
  );
  return {
    azimuthOffsetDeg,
    panoramaUpTiltDeg,
    panoramaUpAzimuthDeg,
  };
}

export function meshQuaternion(
  orientation: SerializedQuaternion,
  baseYawDeg: number,
): THREE.Quaternion {
  return deserializeQuaternion(orientation).multiply(baseYawQuaternion(baseYawDeg));
}

export function beginAlignmentDrag(
  orientation: SerializedQuaternion,
  pointerWorldDirection: THREE.Vector3,
  baseYawDeg: number,
): AlignmentDragState {
  return {
    initialWorldDirection: pointerWorldDirection.clone().normalize(),
    initialMeshQuaternion: meshQuaternion(orientation, baseYawDeg),
  };
}

export function solveAlignmentDrag(
  drag: AlignmentDragState,
  pointerWorldDirection: THREE.Vector3,
  baseYawDeg: number,
  retainedPanoramaUpAzimuthDeg: number,
): { orientation: SerializedQuaternion; controls: OrientationControls } {
  const delta = new THREE.Quaternion().setFromUnitVectors(
    drag.initialWorldDirection,
    pointerWorldDirection.clone().normalize(),
  );
  const candidateMesh = delta.multiply(drag.initialMeshQuaternion.clone()).normalize();
  const candidatePose = candidateMesh.multiply(baseYawQuaternion(baseYawDeg).invert()).normalize();
  const orientation = serializeQuaternion(candidatePose);
  const controls = controlsFromOrientation(orientation, retainedPanoramaUpAzimuthDeg);
  return { orientation, controls };
}

export function beginRollDrag(
  orientation: SerializedQuaternion,
  worldAxis: THREE.Vector3,
): RollDragState {
  return {
    axis: worldAxis.clone().normalize(),
    initialOrientation: deserializeQuaternion(orientation),
  };
}

export function solveRollDrag(
  drag: RollDragState,
  angleRad: number,
  retainedPanoramaUpAzimuthDeg: number,
): { orientation: SerializedQuaternion; controls: OrientationControls } {
  const delta = new THREE.Quaternion().setFromAxisAngle(drag.axis, angleRad);
  const orientation = serializeQuaternion(delta.multiply(drag.initialOrientation.clone()));
  return {
    orientation,
    controls: controlsFromOrientation(orientation, retainedPanoramaUpAzimuthDeg),
  };
}

export function projectDirectionToViewport(
  worldDirection: THREE.Vector3,
  camera: THREE.PerspectiveCamera,
  width: number,
  height: number,
): ViewportProjection | null {
  if (width <= 0 || height <= 0 || worldDirection.lengthSq() <= EPSILON) return null;

  camera.updateMatrixWorld(true);
  const worldPoint = worldDirection.clone().normalize().multiplyScalar(500);
  const cameraForward = new THREE.Vector3();
  camera.getWorldDirection(cameraForward);
  if (worldPoint.clone().sub(camera.position).dot(cameraForward) <= 0) return null;

  const projected = worldPoint.project(camera);
  if (
    projected.z < -1
    || projected.z > 1
    || Math.abs(projected.x) > 1
    || Math.abs(projected.y) > 1
  ) {
    return null;
  }
  return {
    leftPx: (projected.x + 1) * 0.5 * width,
    topPx: (1 - projected.y) * 0.5 * height,
  };
}

export function legacyEulerOrientation(
  yawDeg: number,
  pitchDeg: number,
  rollDeg: number,
  baseYawDeg: number,
): SerializedQuaternion {
  const legacyMesh = new THREE.Quaternion().setFromEuler(
    new THREE.Euler(
      THREE.MathUtils.degToRad(rollDeg),
      THREE.MathUtils.degToRad(baseYawDeg + yawDeg),
      THREE.MathUtils.degToRad(pitchDeg),
      "YXZ",
    ),
  );
  return serializeQuaternion(legacyMesh.multiply(baseYawQuaternion(baseYawDeg).invert()));
}

export function serializeQuaternion(quaternion: THREE.Quaternion): SerializedQuaternion {
  const normalized = quaternion.clone().normalize();
  const sign = normalized.w < 0 ? -1 : 1;
  return {
    w: normalized.w * sign,
    x: normalized.x * sign,
    y: normalized.y * sign,
    z: normalized.z * sign,
  };
}

export function deserializeQuaternion(value: SerializedQuaternion): THREE.Quaternion {
  const quaternion = new THREE.Quaternion(value.x, value.y, value.z, value.w);
  return quaternion.lengthSq() > EPSILON ? quaternion.normalize() : new THREE.Quaternion();
}

function baseYawQuaternion(baseYawDeg: number): THREE.Quaternion {
  return new THREE.Quaternion().setFromAxisAngle(
    WORLD_UP,
    THREE.MathUtils.degToRad(baseYawDeg),
  );
}

function panoramaUpSwing(tiltRad: number, panoramaUpAzimuthDeg: number): THREE.Quaternion {
  const azimuth = THREE.MathUtils.degToRad(normalizeDegrees(panoramaUpAzimuthDeg));
  const axis = new THREE.Vector3(Math.cos(azimuth), 0, -Math.sin(azimuth));
  return new THREE.Quaternion().setFromAxisAngle(axis, tiltRad);
}

function normalizeDegrees(value: number): number {
  return ((value % 360) + 360) % 360;
}
