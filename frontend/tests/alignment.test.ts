import assert from "node:assert/strict";
import test from "node:test";
import * as THREE from "three";

import {
  beginAlignmentDrag,
  beginRollDrag,
  controlsFromOrientation,
  legacyEulerOrientation,
  meshQuaternion,
  orientationFromControls,
  projectDirectionToViewport,
  solveAlignmentDrag,
  solveRollDrag,
  type OrientationControls,
} from "../src/alignment.ts";

const BASE_YAW_DEG = -90;
const EPSILON = 1e-10;

function assertQuaternionEquivalent(actual: THREE.Quaternion, expected: THREE.Quaternion): void {
  assert.ok(1 - Math.abs(actual.dot(expected)) < EPSILON, `${actual.toArray()} != ${expected.toArray()}`);
}

function direction(x: number, y: number, z: number): THREE.Vector3 {
  return new THREE.Vector3(x, y, z).normalize();
}

test("orientation controls round-trip without depending on viewing azimuth", () => {
  const cases: OrientationControls[] = [
    { azimuthOffsetDeg: 0, panoramaUpTiltDeg: 0, panoramaUpAzimuthDeg: 217 },
    { azimuthOffsetDeg: 30, panoramaUpTiltDeg: 30, panoramaUpAzimuthDeg: 0 },
    { azimuthOffsetDeg: 125, panoramaUpTiltDeg: 90, panoramaUpAzimuthDeg: 90 },
    { azimuthOffsetDeg: 275, panoramaUpTiltDeg: 120, panoramaUpAzimuthDeg: 241 },
    { azimuthOffsetDeg: 42, panoramaUpTiltDeg: 179, panoramaUpAzimuthDeg: 315 },
    { azimuthOffsetDeg: 81, panoramaUpTiltDeg: 180, panoramaUpAzimuthDeg: 123 },
  ];

  for (const controls of cases) {
    const orientation = orientationFromControls(controls);
    const recovered = controlsFromOrientation(orientation, controls.panoramaUpAzimuthDeg);
    assertQuaternionEquivalent(
      meshQuaternion(orientationFromControls(recovered), BASE_YAW_DEG),
      meshQuaternion(orientation, BASE_YAW_DEG),
    );
    assert.ok(Math.abs(recovered.panoramaUpTiltDeg - controls.panoramaUpTiltDeg) < EPSILON);
    if (controls.panoramaUpTiltDeg === 0 || controls.panoramaUpTiltDeg === 180) {
      assert.equal(recovered.panoramaUpAzimuthDeg, controls.panoramaUpAzimuthDeg);
    }
  }
});

test("an upside-down quaternion survives decomposition with a different retained pole azimuth", () => {
  const orientation = orientationFromControls({
    azimuthOffsetDeg: 81,
    panoramaUpTiltDeg: 180,
    panoramaUpAzimuthDeg: 123,
  });
  const recovered = controlsFromOrientation(orientation, 17);

  assert.equal(recovered.panoramaUpAzimuthDeg, 17);
  assertQuaternionEquivalent(
    meshQuaternion(orientationFromControls(recovered), BASE_YAW_DEG),
    meshQuaternion(orientation, BASE_YAW_DEG),
  );
});

test("panorama-up azimuth is opposite the high side of the horizon below ninety degrees", () => {
  const tiltDeg = 35;
  const panoramaUpAzimuthDeg = 270;
  const orientation = orientationFromControls({
    azimuthOffsetDeg: 0,
    panoramaUpTiltDeg: tiltDeg,
    panoramaUpAzimuthDeg,
  });
  const normal = new THREE.Vector3(0, 1, 0).applyQuaternion(
    new THREE.Quaternion(orientation.x, orientation.y, orientation.z, orientation.w),
  );
  const azimuth = THREE.MathUtils.degToRad((panoramaUpAzimuthDeg + 180) % 360);
  const altitude = THREE.MathUtils.degToRad(tiltDeg);
  const highHorizonPoint = new THREE.Vector3(
    Math.cos(altitude) * Math.sin(azimuth),
    Math.sin(altitude),
    Math.cos(altitude) * Math.cos(azimuth),
  );
  assert.ok(Math.abs(normal.dot(highHorizonPoint)) < EPSILON);
});

test("a diagonal Align Target drag keeps the grabbed source point under the cursor", () => {
  const starts = [
    direction(0, 0, -1),
    direction(1, 0, 0),
    direction(-1, 0, 0),
    direction(0.4, 0.2, -1),
  ];
  const ends = [
    direction(0.25, 0.18, -1),
    direction(1, 0.22, -0.28),
    direction(-1, -0.18, -0.25),
    direction(0.62, -0.05, -1),
  ];
  const initial = orientationFromControls({
    azimuthOffsetDeg: 23,
    panoramaUpTiltDeg: 16,
    panoramaUpAzimuthDeg: 312,
  });

  for (let index = 0; index < starts.length; index += 1) {
    const initialMesh = meshQuaternion(initial, BASE_YAW_DEG);
    const grabbedLocalPoint = starts[index].clone().applyQuaternion(initialMesh.clone().invert());
    const drag = beginAlignmentDrag(initial, starts[index], BASE_YAW_DEG);
    const solved = solveAlignmentDrag(drag, ends[index], BASE_YAW_DEG, 132);
    const solvedWorldPoint = grabbedLocalPoint.applyQuaternion(
      meshQuaternion(solved.orientation, BASE_YAW_DEG),
    );
    assert.ok(solvedWorldPoint.angleTo(ends[index]) < EPSILON);
  }
});

test("Align Target applies exactly the shortest-arc cursor rotation", () => {
  const start = direction(0.2, -0.1, -1);
  const end = direction(-0.25, 0.3, -1);
  const initial = orientationFromControls({
    azimuthOffsetDeg: 76,
    panoramaUpTiltDeg: 12,
    panoramaUpAzimuthDeg: 130,
  });
  const drag = beginAlignmentDrag(initial, start, BASE_YAW_DEG);
  const solved = solveAlignmentDrag(drag, end, BASE_YAW_DEG, 310);
  const expected = new THREE.Quaternion()
    .setFromUnitVectors(start, end)
    .multiply(drag.initialMeshQuaternion.clone());

  assertQuaternionEquivalent(meshQuaternion(solved.orientation, BASE_YAW_DEG), expected);
});

test("roll keeps its visual-center axis fixed at any altitude", () => {
  const initial = orientationFromControls({
    azimuthOffsetDeg: 0,
    panoramaUpTiltDeg: 0,
    panoramaUpAzimuthDeg: 90,
  });
  const axis = direction(1, 0.4, 1);
  const drag = beginRollDrag(initial, axis);
  const solved = solveRollDrag(drag, THREE.MathUtils.degToRad(90), 90);

  assert.ok(axis.angleTo(axis.clone().applyQuaternion(
    new THREE.Quaternion(
      solved.orientation.x,
      solved.orientation.y,
      solved.orientation.z,
      solved.orientation.w,
    ),
  )) < EPSILON);
});

test("roll about a horizontal center ray passes through sideways to upside down", () => {
  const initial = orientationFromControls({
    azimuthOffsetDeg: 0,
    panoramaUpTiltDeg: 0,
    panoramaUpAzimuthDeg: 90,
  });
  const drag = beginRollDrag(initial, direction(1, 0, 1));
  const sideways = solveRollDrag(drag, THREE.MathUtils.degToRad(90), 90);
  const inverted = solveRollDrag(drag, THREE.MathUtils.degToRad(180), 90);

  assert.ok(Math.abs(sideways.controls.panoramaUpTiltDeg - 90) < EPSILON);
  assert.ok(Math.abs(inverted.controls.panoramaUpTiltDeg - 180) < EPSILON);
});

test("roll is solved from the drag-start orientation rather than accumulated deltas", () => {
  const initial = orientationFromControls({
    azimuthOffsetDeg: 37,
    panoramaUpTiltDeg: 64,
    panoramaUpAzimuthDeg: 212,
  });
  const axis = direction(-0.3, 0, -1);
  const drag = beginRollDrag(initial, axis);
  const angle = THREE.MathUtils.degToRad(73);
  const solved = solveRollDrag(drag, angle, 212);
  const expected = new THREE.Quaternion()
    .setFromAxisAngle(axis, angle)
    .multiply(new THREE.Quaternion(initial.x, initial.y, initial.z, initial.w));

  assertQuaternionEquivalent(
    new THREE.Quaternion(
      solved.orientation.x,
      solved.orientation.y,
      solved.orientation.z,
      solved.orientation.w,
    ),
    expected,
  );
});

test("successive roll gestures preserve the same pinned world direction", () => {
  const initial = orientationFromControls({
    azimuthOffsetDeg: 18,
    panoramaUpTiltDeg: 41,
    panoramaUpAzimuthDeg: 286,
  });
  const axis = direction(-0.4, 0.7, -1);
  const initialQuaternion = new THREE.Quaternion(initial.x, initial.y, initial.z, initial.w);
  const pinnedSourcePoint = axis.clone().applyQuaternion(initialQuaternion.clone().invert());
  const first = solveRollDrag(
    beginRollDrag(initial, axis),
    THREE.MathUtils.degToRad(47),
    286,
  );
  const second = solveRollDrag(
    beginRollDrag(first.orientation, axis),
    THREE.MathUtils.degToRad(-113),
    first.controls.panoramaUpAzimuthDeg,
  );
  const finalQuaternion = new THREE.Quaternion(
    second.orientation.x,
    second.orientation.y,
    second.orientation.z,
    second.orientation.w,
  );

  assert.ok(pinnedSourcePoint.applyQuaternion(finalQuaternion).angleTo(axis) < EPSILON);
});

test("positive roll around the centered south horizon ray appears clockwise", () => {
  const initial = orientationFromControls({
    azimuthOffsetDeg: 0,
    panoramaUpTiltDeg: 0,
    panoramaUpAzimuthDeg: 0,
  });
  const drag = beginRollDrag(initial, new THREE.Vector3(0, 0, -1));
  const solved = solveRollDrag(drag, THREE.MathUtils.degToRad(30), 0);
  const panoramaUp = new THREE.Vector3(0, 1, 0).applyQuaternion(
    new THREE.Quaternion(
      solved.orientation.x,
      solved.orientation.y,
      solved.orientation.z,
      solved.orientation.w,
    ),
  );

  assert.ok(panoramaUp.x > 0, "the top of the panorama should move toward screen right");
});

test("pivot projection reports centered, off-center, and hidden directions", () => {
  const camera = new THREE.PerspectiveCamera(90, 1, 0.1, 2000);
  camera.position.set(0, 0, 0.01);
  camera.lookAt(0, 0, -1);
  camera.updateProjectionMatrix();
  camera.updateMatrixWorld(true);

  const centered = projectDirectionToViewport(new THREE.Vector3(0, 0, -1), camera, 100, 100);
  const offCenter = projectDirectionToViewport(direction(0.5, 0, -1), camera, 100, 100);

  assert.ok(centered);
  assert.ok(Math.abs(centered.leftPx - 50) < EPSILON);
  assert.ok(Math.abs(centered.topPx - 50) < EPSILON);
  assert.ok(offCenter && offCenter.leftPx > 50 && offCenter.leftPx < 100);
  assert.equal(projectDirectionToViewport(direction(2, 0, -1), camera, 100, 100), null);
  assert.equal(projectDirectionToViewport(new THREE.Vector3(0, 0, 1), camera, 100, 100), null);
});

test("legacy Euler metadata retains its old full-mesh interpretation", () => {
  const legacy = legacyEulerOrientation(30, 20, -7, BASE_YAW_DEG);
  const expected = new THREE.Quaternion().setFromEuler(
    new THREE.Euler(
      THREE.MathUtils.degToRad(-7),
      THREE.MathUtils.degToRad(BASE_YAW_DEG + 30),
      THREE.MathUtils.degToRad(20),
      "YXZ",
    ),
  );
  assertQuaternionEquivalent(meshQuaternion(legacy, BASE_YAW_DEG), expected);
});
