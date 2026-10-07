// Dev-only golden fixture generator. Never shipped, never run by CI or cargo.
//
// Runs the original perfect-freehand 1.2.0 with Excalidraw's freedraw options
// on fixed inputs and writes a plain-text fixture the Rust parity test reads:
//
//   cd tools/fixture-gen && npm install
//   node gen.mjs > ../../crates/core/tests/fixtures/freehand.txt
//   node gen.mjs laser > ../../crates/core/tests/fixtures/laser.txt
//
// (or: PERFECT_FREEHAND=/path/to/perfect-freehand/dist/esm/index.js node gen.mjs)
//
// Options mirror Excalidraw's renderElement.ts freedraw call site:
// size = strokeWidth * 4.25, thinning 0.6, smoothing 0.5, streamline 0.5,
// easeOutSine, last = committed.
const laserMode = process.argv[2] === "laser";
const { getStroke } = await import(process.env.PERFECT_FREEHAND || "perfect-freehand");
const laserLib = laserMode ? await import(process.env.LASER_POINTER || "@excalidraw/laser-pointer") : null;

const options = (strokeWidth, simulatePressure, last) => ({
  simulatePressure,
  size: strokeWidth * 4.25,
  thinning: 0.6,
  smoothing: 0.5,
  streamline: 0.5,
  easing: (t) => Math.sin((t * Math.PI) / 2),
  last,
});

// Deterministic pseudo-random, so fixtures are reproducible.
let seed = 12345;
const rnd = () => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296);

const walk = (n, step, turn) => {
  const pts = [[0, 0]];
  let a = 0;
  for (let i = 1; i < n; i++) {
    a += (rnd() - 0.5) * turn;
    const [x, y] = pts[i - 1];
    pts.push([x + Math.cos(a) * step * (0.3 + rnd()), y + Math.sin(a) * step * (0.3 + rnd())]);
  }
  return pts;
};
const sine = Array.from({ length: 40 }, (_, i) => [i * 3.7, 20 * Math.sin(i / 5) + (i % 3) * 0.3]);
const zigzag = Array.from({ length: 12 }, (_, i) => [i * 9, i % 2 ? 30 : 0]);
const loop = Array.from({ length: 30 }, (_, i) => [50 + 40 * Math.cos(i / 4.7), 50 + 40 * Math.sin(i / 4.7)]);
const longWalk = walk(400, 4, 0.5);
const pressures = (pts, f) => pts.map((_, i) => f(i));
const wave = (i) => 0.2 + 0.6 * Math.abs(Math.sin(i / 7));

// [name, points, pressures | null (= simulate), strokeWidth, committed]
const cases = [
  ["sine_sim_live", sine, null, 2, false],
  ["sine_sim_done", sine, null, 2, true],
  ["sine_pressure_live", sine, pressures(sine, wave), 2, false],
  ["sine_pressure_done", sine, pressures(sine, wave), 2, true],
  ["sine_thin", sine, null, 1, true],
  ["sine_bold", sine, null, 4, true],
  ["zigzag_sharp_done", zigzag, null, 2, true],
  ["zigzag_pressure_done", zigzag, pressures(zigzag, (i) => (i % 5) / 5), 2, true],
  ["loop_done", loop, null, 2, true],
  ["dot_done", [[0, 0], [0.0001, 0.0001]], null, 2, true],
  ["dot_live", [[0, 0]], null, 2, false],
  ["one_point_done", [[0, 0]], null, 2, true],
  ["two_points_done", [[0, 0], [30, 10]], null, 2, true],
  ["two_points_pressure", [[0, 0], [30, 10]], [0.3, 0.9], 2, true],
  ["short_done", [[0, 0], [1.5, 0.5], [2.5, 1], [3, 2]], null, 2, true],
  ["walk_slow_done", walk(120, 1.2, 0.8), null, 2, true],
  ["walk_fast_done", walk(60, 25, 0.6), null, 2, true],
  ["walk_long_live", longWalk, null, 2, false],
  ["walk_long_pressure_done", longWalk, pressures(longWalk, (i) => 0.1 + 0.8 * Math.abs(Math.sin(i / 11))), 2, true],
];

// ---- Laser: @excalidraw/laser-pointer 1.3.1 with Excalidraw's trail options
// (laser-trails.ts getTrailOptions). Points carry their timestamp (ms) as the
// third coordinate, like Excalidraw's `performance.now()`; `now` replaces the
// `performance.now()` inside sizeMapping so every case is deterministic.
const easeOut = (k) => 1 - Math.pow(1 - k, 4);
const laserOutline = (pts, now, close) => {
  const trail = new (laserLib.LaserPointer ?? laserLib.default.LaserPointer)({
    simplify: 0,
    streamline: 0.4,
    sizeMapping: (c) => {
      const DECAY_TIME = 1000;
      const DECAY_LENGTH = 50;
      const t = Math.max(0, 1 - (now - c.pressure) / DECAY_TIME);
      const l = (DECAY_LENGTH - Math.min(DECAY_LENGTH, c.totalLength - c.currentIndex)) / DECAY_LENGTH;
      return Math.min(easeOut(l), easeOut(t));
    },
  });
  pts.forEach(([x, y, t]) => trail.addPoint([x, y, t]));
  if (close) trail.close();
  return trail.getStrokeOutline(2);
};
const stamp = (pts, start = 0, step = 10) => pts.map(([x, y], i) => [x, y, start + i * step]);
const lastT = (pts) => pts[pts.length - 1][2];

const out = [];
if (laserMode) {
  const lsine = stamp(sine);
  const lwalk = stamp(walk(160, 6, 0.9));
  const lfast = stamp(walk(30, 40, 1.2));
  // [name, points with ms, now, closed (pointer up)]
  const laserCases = [
    ["one_point", stamp([[10, 10]]), 50, false],
    ["one_point_faded", stamp([[10, 10]]), 900, false],
    ["two_points", stamp([[10, 10], [40, 25]]), 60, false],
    ["three_points", stamp([[0, 0], [30, 5], [60, 30]]), 80, false],
    ["duplicate_points", stamp([[0, 0], [0, 0], [20, 5], [20, 5], [45, 20], [70, 20]]), 100, false],
    ["sine_fresh", lsine, lastT(lsine) + 16, false],
    ["sine_closed", lsine, lastT(lsine) + 16, true],
    ["sine_mid_fade", lsine, lastT(lsine) + 500, true],
    ["sine_late_fade", lsine, lastT(lsine) + 900, true],
    ["sine_dead", lsine, lastT(lsine) + 1500, true],
    ["zigzag_corners", stamp(zigzag, 0, 8), 120, true],
    ["zigzag_live", stamp(zigzag, 0, 8), 120, false],
    ["walk_long_live", lwalk, lastT(lwalk) + 16, false],
    ["walk_long_closed", lwalk, lastT(lwalk) + 16, true],
    ["walk_long_mid_fade", lwalk, lastT(lwalk) + 600, true],
    ["walk_fast", lfast, lastT(lfast) + 40, true],
    ["walk_fast_live_late", lfast, lastT(lfast) + 700, false],
  ];
  for (const [name, pts, now, close] of laserCases) {
    out.push(`case ${name} now=${now} close=${close ? 1 : 0}`);
    pts.forEach((p) => out.push(`in ${p.join(" ")}`));
    laserOutline(pts, now, close).forEach(([x, y]) => out.push(`out ${x} ${y}`));
  }
  console.log(out.join("\n"));
  process.exit(0);
}
for (const [name, pts, press, width, last] of cases) {
  const sim = press === null;
  const input = sim ? pts : pts.map(([x, y], i) => [x, y, press[i]]);
  const outline = getStroke(input, options(width, sim, last));
  out.push(`case ${name} simulate=${sim ? 1 : 0} size=${width * 4.25} last=${last ? 1 : 0}`);
  pts.forEach(([x, y], i) => out.push(`in ${x} ${y}${sim ? "" : " " + press[i]}`));
  outline.forEach(([x, y]) => out.push(`out ${x} ${y}`));
}
console.log(out.join("\n"));
