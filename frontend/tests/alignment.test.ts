import assert from "node:assert/strict";
import test from "node:test";
import * as THREE from "three";

import {
  beginAlignmentDrag,
  controlsFromOrientation,
  legacyEulerOrientation,
  meshQuaternion,
  orientationFromControls,
  solveAlignmentDrag,
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
    { azimuthOffsetDeg: 0, tiltAngleDeg: 0, highSideAzimuthDeg: 217 },
    { azimuthOffsetDeg: 30, tiltAngleDeg: 30, highSideAzimuthDeg: 0 },
    { azimuthOffsetDeg: 125, tiltAngleDeg: 42, highSideAzimuthDeg: 90 },
    { azimuthOffsetDeg: 275, tiltAngleDeg: 67, highSideAzimuthDeg: 241 },
  ];

  for (const controls of cases) {
    const orientation = orientationFromControls(controls);
    const recovered = controlsFromOrientation(orientation, controls.highSideAzimuthDeg);
    assertQuaternionEquivalent(
      meshQuaternion(orientationFromControls(recovered), BASE_YAW_DEG),
      meshQuaternion(orientation, BASE_YAW_DEG),
    );
    assert.ok(Math.abs(recovered.tiltAngleDeg - controls.tiltAngleDeg) < EPSILON);
    if (controls.tiltAngleDeg === 0) {
      assert.equal(recovered.highSideAzimuthDeg, controls.highSideAzimuthDeg);
    }
  }
});

test("high-side azimuth identifies where the panorama horizon reaches its highest altitude", () => {
  const tiltDeg = 35;
  const highSideAzimuthDeg = 90;
  const orientation = orientationFromControls({
    azimuthOffsetDeg: 0,
    tiltAngleDeg: tiltDeg,
    highSideAzimuthDeg,
  });
  const normal = new THREE.Vector3(0, 1, 0).applyQuaternion(
    new THREE.Quaternion(orientation.x, orientation.y, orientation.z, orientation.w),
  );
  const azimuth = THREE.MathUtils.degToRad(highSideAzimuthDeg);
  const altitude = THREE.MathUtils.degToRad(tiltDeg);
  const highHorizonPoint = new THREE.Vector3(
    -Math.cos(altitude) * Math.sin(azimuth),
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
    tiltAngleDeg: 16,
    highSideAzimuthDeg: 132,
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
    tiltAngleDeg: 12,
    highSideAzimuthDeg: 310,
  });
  const drag = beginAlignmentDrag(initial, start, BASE_YAW_DEG);
  const solved = solveAlignmentDrag(drag, end, BASE_YAW_DEG, 310);
  const expected = new THREE.Quaternion()
    .setFromUnitVectors(start, end)
    .multiply(drag.initialMeshQuaternion.clone());

  assertQuaternionEquivalent(meshQuaternion(solved.orientation, BASE_YAW_DEG), expected);
});

test("tilt is constrained to the supported zero-to-ninety-degree range", () => {
  const orientation = orientationFromControls({
    azimuthOffsetDeg: 0,
    tiltAngleDeg: 140,
    highSideAzimuthDeg: 45,
  });
  const recovered = controlsFromOrientation(orientation, 45);
  assert.ok(Math.abs(recovered.tiltAngleDeg - 90) < EPSILON);
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
