// Process-start measurements, not idle frame pacing. Run inside xvfb-run on headless Linux.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const build = path.join(root, '.build');
await fs.mkdir(build, { recursive: true });
const scratch = await fs.mkdtemp(path.join(build, 'startup-benchmark-'));
const binary =
  process.env.LUMA_STARTUP_BINARY ??
  path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'release/luma');
const runs = Number(process.env.LUMA_STARTUP_RUNS ?? 5);
if (!Number.isInteger(runs) || runs < 1 || runs > 20)
  throw new Error('LUMA_STARTUP_RUNS must be 1–20');
const samples = { runtime: [], prepared: [] };
const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
try {
  const preferences = path.join(scratch, 'preferences.json');
  await fs.writeFile(
    preferences,
    JSON.stringify({ show_weather: false, reduced_motion: true, background: 'midnight' }),
  );
  for (let run = 0; run < runs; run++) {
    for (const mode of ['runtime', 'prepared']) {
      const report = path.join(scratch, `${mode}-${run}.json`);
      const result = spawnSync(
        binary,
        [
          '--assets',
          path.join(build, 'assets'),
          '--preferences',
          preferences,
          '--script',
          'home',
          '--frames',
          '12',
          '--time',
          '2026-10-22T20:41:00+02:00',
          '--diagnostics',
          report,
          '--weather-test-server',
          'http://127.0.0.1:1',
        ],
        {
          cwd: root,
          env: { ...process.env, LUMA_GLASS_PRECOMPUTED: mode === 'runtime' ? '0' : '1' },
          encoding: 'utf8',
          timeout: 60000,
        },
      );
      if (result.error) throw result.error;
      if (result.status !== 0) throw new Error(result.stderr);
      const { startup } = JSON.parse(await fs.readFile(report, 'utf8'));
      if (mode === 'prepared' && startup.first_frame.glass_precomputed_loads !== 1) {
        throw new Error('Expected a precomputed glass atlas. Run pnpm assets before benchmarking.');
      }
      samples[mode].push(startup);
    }
  }
  const summary = Object.fromEntries(
    Object.entries(samples).map(([mode, values]) => [
      mode,
      {
        first_frame_median_ms: median(values.map((sample) => sample.stages_ms['first frame'])),
        glass_prepare_median_ms: median(
          values.map((sample) => sample.first_frame.glass_prepare_ms),
        ),
      },
    ]),
  );
  await fs.writeFile(
    path.join(build, 'startup-benchmark.json'),
    JSON.stringify({ summary, samples }, null, 2),
  );
  console.log(JSON.stringify(summary, null, 2));
  console.log(
    'Desktop process starts with warm filesystem/driver caches, not TV cold-boot timings.',
  );
} finally {
  await fs.rm(scratch, { recursive: true, force: true });
}
