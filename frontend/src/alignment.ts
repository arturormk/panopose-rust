import * as THREE from "three";

export interface SerializedQuaternion {
  w: number;
  x: number;
  y: number;
  z: number;
}

export interface OrientationControls {
  azimuthOffsetDeg: number;
  tiltAngleDeg: number;
  highSideAzimuthDeg: number;
}

export interface AlignmentDragState {
  initialWorldDirection: THREE.Vector3;
  initialMeshQuaternion: THREE.Quaternion;
}

const WORLD_UP = new THREE.Vector3(0, 1, 0);
const MAX_TILT_DEG = 90;
const EPSILON = 1e-10;

export const IDENTITY_ORIENTATION: SerializedQuaternion = { w: 1, x: 0, y: 0, z: 0 };

export function orientationFromControls(controls: OrientationControls): SerializedQuaternion {
  const tilt = THREE.MathUtils.degToRad(THREE.MathUtils.clamp(controls.tiltAngleDeg, 0, MAX_TILT_DEG));
  const highAzimuth = THREE.MathUtils.degToRad(normalizeDegrees(controls.highSideAzimuthDeg));
  const highDirection = new THREE.Vector3(-Math.sin(highAzimuth), 0, Math.cos(highAzimuth));
  const horizonNormal = WORLD_UP.clone()
    .multiplyScalar(Math.cos(tilt))
    .addScaledVector(highDirection, -Math.sin(tilt))
    .normalize();
  const swing = new THREE.Quaternion().setFromUnitVectors(WORLD_UP, horizonNormal);
  const twist = new THREE.Quaternion().setFromAxisAngle(
    WORLD_UP,
    THREE.MathUtils.degToRad(controls.azimuthOffsetDeg),
  );
  return serializeQuaternion(swing.multiply(twist));
}

export function controlsFromOrientation(
  orientation: SerializedQuaternion,
  retainedHighSideAzimuthDeg: number,
): OrientationControls {
  const quaternion = deserializeQuaternion(orientation);
  const horizonNormal = WORLD_UP.clone().applyQuaternion(quaternion).normalize();
  const tiltAngleDeg = THREE.MathUtils.radToDeg(
    Math.acos(THREE.MathUtils.clamp(horizonNormal.y, -1, 1)),
  );
  const horizontalLength = Math.hypot(horizonNormal.x, horizonNormal.z);
  const highSideAzimuthDeg =
    horizontalLength <= EPSILON
      ? normalizeDegrees(retainedHighSideAzimuthDeg)
      : normalizeDegrees(
          THREE.MathUtils.radToDeg(Math.atan2(horizonNormal.x, -horizonNormal.z)),
        );
  const swing = new THREE.Quaternion().setFromUnitVectors(WORLD_UP, horizonNormal);
  const twist = swing.invert().multiply(quaternion).normalize();
  const azimuthOffsetDeg = normalizeDegrees(
    THREE.MathUtils.radToDeg(2 * Math.atan2(twist.y, twist.w)),
  );
  return {
    azimuthOffsetDeg,
    tiltAngleDeg: Math.min(tiltAngleDeg, MAX_TILT_DEG),
    highSideAzimuthDeg,
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
  retainedHighSideAzimuthDeg: number,
): { orientation: SerializedQuaternion; controls: OrientationControls } {
  const delta = new THREE.Quaternion().setFromUnitVectors(
    drag.initialWorldDirection,
    pointerWorldDirection.clone().normalize(),
  );
  const candidateMesh = delta.multiply(drag.initialMeshQuaternion.clone()).normalize();
  const candidatePose = candidateMesh.multiply(baseYawQuaternion(baseYawDeg).invert()).normalize();
  let orientation = serializeQuaternion(candidatePose);
  let controls = controlsFromOrientation(orientation, retainedHighSideAzimuthDeg);
  if (controls.tiltAngleDeg >= MAX_TILT_DEG) {
    controls = { ...controls, tiltAngleDeg: MAX_TILT_DEG };
    orientation = orientationFromControls(controls);
  }
  return { orientation, controls };
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

function normalizeDegrees(value: number): number {
  return ((value % 360) + 360) % 360;
}
