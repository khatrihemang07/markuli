// Dev-only golden fixture generator. Never shipped, never run by CI or cargo.
//
// Runs the original perfect-freehand 1.2.0 with Excalidraw's freedraw options
// on fixed inputs and writes a plain-text fixture the Rust parity test reads:
//
//   cd tools/fixture-gen && npm install
//   node gen.mjs > ../../crates/core/tests/fixtures/freehand.txt
//
// (or: PERFECT_FREEHAND=/path/to/perfect-freehand/dist/esm/index.js node gen.mjs)
//
// Options mirror Excalidraw's renderElement.ts freedraw call site:
// size = strokeWidth * 4.25, thinning 0.6, smoothing 0.5, streamline 0.5,
// easeOutSine, last = committed.
const { getStroke } = await import(process.env.PERFECT_FREEHAND || "perfect-freehand");

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

const out = [];
for (const [name, pts, press, width, last] of cases) {
  const sim = press === null;
  const input = sim ? pts : pts.map(([x, y], i) => [x, y, press[i]]);
  const outline = getStroke(input, options(width, sim, last));
  out.push(`case ${name} simulate=${sim ? 1 : 0} size=${width * 4.25} last=${last ? 1 : 0}`);
  pts.forEach(([x, y], i) => out.push(`in ${x} ${y}${sim ? "" : " " + press[i]}`));
  outline.forEach(([x, y]) => out.push(`out ${x} ${y}`));
}
console.log(out.join("\n"));
