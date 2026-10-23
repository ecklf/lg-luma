import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { syncVersions } from '../tools/sync-version.mjs';
import { isReleaseMerge, isVersionIncrease } from '../tools/release-gate.mjs';

const root = path.resolve(import.meta.dirname, '..');
const metadata = ['appinfo.json', 'Cargo.toml', 'Cargo.lock', 'readme.md'];

async function fixture(t, version = '2.0.4') {
  const scratch = path.join(root, '.build');
  await fs.mkdir(scratch, { recursive: true });
  const directory = await fs.mkdtemp(path.join(scratch, 'release-test-'));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const project = JSON.parse(await fs.readFile(path.join(root, 'package.json'), 'utf8'));
  project.version = '1.2.3';
  await fs.writeFile(path.join(directory, 'package.json'), JSON.stringify(project));
  await Promise.all(
    metadata.map((filename) =>
      fs.copyFile(path.join(root, filename), path.join(directory, filename)),
    ),
  );
  await syncVersions(directory);
  project.version = version;
  await fs.writeFile(path.join(directory, 'package.json'), JSON.stringify(project));
  return directory;
}

const readMetadata = (directory) =>
  Promise.all(metadata.map((filename) => fs.readFile(path.join(directory, filename), 'utf8')));

test('version sync changes only release versions and is idempotent', async (t) => {
  const directory = await fixture(t, '3.1.0');
  const before = await readMetadata(directory);
  await assert.rejects(syncVersions(directory, { check: true }), /out of sync/);
  assert.deepEqual(await readMetadata(directory), before, 'check mode must not write files');

  assert.deepEqual((await syncVersions(directory)).changed, metadata);
  const after = await readMetadata(directory);
  const previous = JSON.parse(before[0]).version;
  assert.equal(JSON.parse(after[0]).version, '3.1.0');
  assert.equal(after[0], before[0].replace(`"version": "${previous}"`, '"version": "3.1.0"'));
  assert.equal(after[1], before[1].replace(`version = "${previous}"`, 'version = "3.1.0"'));
  assert.equal(
    after[2],
    before[2].replace(`name = "luma"\nversion = "${previous}"`, 'name = "luma"\nversion = "3.1.0"'),
    'dependency versions and helper crates must be preserved',
  );
  assert.equal(
    after[3],
    before[3].replaceAll(`com.ecklf.luma_${previous}_arm.ipk`, 'com.ecklf.luma_3.1.0_arm.ipk'),
  );
  assert.deepEqual((await syncVersions(directory, { check: true })).changed, []);
  assert.deepEqual((await syncVersions(directory)).changed, []);
});

test('invalid release versions and missing packages fail before writing metadata', async (t) => {
  for (const version of ['3.0.0-beta.1', '3.0', '03.0.0']) {
    const directory = await fixture(t, version);
    const before = await readMetadata(directory);
    await assert.rejects(syncVersions(directory), /numeric major.minor.patch/);
    assert.deepEqual(await readMetadata(directory), before);
  }
  const directory = await fixture(t);
  const lockfile = path.join(directory, 'Cargo.lock');
  await fs.writeFile(
    lockfile,
    (await fs.readFile(lockfile, 'utf8')).replace('name = "luma"', 'name = "other"'),
  );
  const before = await readMetadata(directory);
  await assert.rejects(syncVersions(directory), /one luma package in Cargo.lock/);
  assert.deepEqual(await readMetadata(directory), before);
});

test('the real release command combines changesets and synchronizes every file', async (t) => {
  const directory = await fixture(t, '1.2.3');
  await syncVersions(directory);
  await fs.mkdir(path.join(directory, 'tools'));
  await fs.mkdir(path.join(directory, '.changeset'));
  for (const filename of [
    'tools/sync-version.mjs',
    '.changeset/config.json',
    'pnpm-workspace.yaml',
    '.oxfmtrc.json',
  ]) {
    await fs.copyFile(path.join(root, filename), path.join(directory, filename));
  }
  await fs.symlink(path.join(root, 'node_modules'), path.join(directory, 'node_modules'), 'dir');
  await fs.writeFile(
    path.join(directory, '.changeset/fix.md'),
    '---\n"lg-luma": patch\n---\n\nFix release behavior.\n',
  );
  await fs.writeFile(
    path.join(directory, '.changeset/feature.md'),
    '---\n"lg-luma": minor\n---\n\nAdd a release feature.\n',
  );
  const result = spawnSync('pnpm', ['version:release'], {
    cwd: directory,
    encoding: 'utf8',
    timeout: 30000,
    env: { ...process.env, CI: 'true' },
  });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  assert.equal((await syncVersions(directory, { check: true })).version, '1.3.0');
  const changelog = await fs.readFile(path.join(directory, 'CHANGELOG.md'), 'utf8');
  assert.match(changelog, /1\.3\.0/);
  assert.match(changelog, /Fix release behavior/);
  assert.match(changelog, /Add a release feature/);
  assert.deepEqual(await fs.readdir(path.join(directory, '.changeset')), ['config.json']);
});

function gateFixture({ pull = {}, previous = '2.0.3', current = '2.0.4', event = {} } = {}) {
  const context = {
    eventName: 'push',
    ref: 'refs/heads/main',
    repo: { owner: 'ecklf', repo: 'lg-luma' },
    sha: 'merged-commit',
    payload: { before: 'previous-commit', forced: false, deleted: false },
    ...event,
  };
  const releasePull = {
    merged_at: '2026-10-23T12:00:00Z',
    merge_commit_sha: context.sha,
    base: { ref: 'main', repo: { full_name: 'ecklf/lg-luma' } },
    head: { ref: 'changeset-release/main', repo: { full_name: 'ecklf/lg-luma' } },
    ...pull,
  };
  const reads = [];
  const github = {
    paginate: async (_method, parameters) => {
      assert.equal(parameters.commit_sha, context.sha);
      return [releasePull];
    },
    rest: {
      repos: {
        listPullRequestsAssociatedWithCommit() {},
        async getContent({ ref, path: filename }) {
          assert.equal(filename, 'package.json');
          reads.push(ref);
          const version = ref === context.sha ? current : previous;
          return {
            data: {
              encoding: 'base64',
              content: Buffer.from(JSON.stringify({ version })).toString('base64'),
            },
          };
        },
      },
    },
  };
  return { github, context, reads };
}

test('a merged same-repository version PR with an increased version can publish', async () => {
  const fixture = gateFixture();
  assert.equal(await isReleaseMerge(fixture), true);
  assert.deepEqual(fixture.reads, ['previous-commit', 'merged-commit']);
});

test('ordinary merges, unmerged PRs, forks and manual runs cannot publish', async (t) => {
  const cases = [
    ['feature PR', { pull: { head: { ref: 'feature', repo: { full_name: 'ecklf/lg-luma' } } } }],
    ['unmerged version PR', { pull: { merged_at: null } }],
    ['different merge commit', { pull: { merge_commit_sha: 'other-commit' } }],
    [
      'fork version branch',
      { pull: { head: { ref: 'changeset-release/main', repo: { full_name: 'other/lg-luma' } } } },
    ],
    ['deleted head repository', { pull: { head: { ref: 'changeset-release/main', repo: null } } }],
    [
      'different base',
      { pull: { base: { ref: 'develop', repo: { full_name: 'ecklf/lg-luma' } } } },
    ],
    ['manual CI run', { event: { eventName: 'workflow_dispatch' } }],
    ['PR checks', { event: { eventName: 'pull_request' } }],
    ['other branch', { event: { ref: 'refs/heads/changeset-release/main' } }],
    ['force push', { event: { payload: { before: 'previous-commit', forced: true } } }],
    [
      'initial push',
      { event: { payload: { before: '0000000000000000000000000000000000000000' } } },
    ],
  ];
  for (const [name, options] of cases) {
    await t.test(name, async () => {
      const fixture = gateFixture(options);
      assert.equal(await isReleaseMerge(fixture), false);
      assert.deepEqual(fixture.reads, []);
    });
  }
});

test('publishing requires a numeric version increase', async () => {
  for (const current of ['2.0.3', '2.0.2', '1.99.99']) {
    assert.equal(await isReleaseMerge(gateFixture({ current })), false);
  }
  for (const [previous, current] of [
    ['2.0.3', '2.0.4'],
    ['2.9.9', '2.10.0'],
    ['2.99.99', '3.0.0'],
  ]) {
    assert.equal(isVersionIncrease(previous, current), true);
  }
  await assert.rejects(
    isReleaseMerge(gateFixture({ current: '3.0.0-beta.1' })),
    /Invalid numeric release version/,
  );
});

test('missing PRs and failed GitHub lookups never permit publication', async () => {
  const fixture = gateFixture();
  fixture.github.paginate = async () => [];
  assert.equal(await isReleaseMerge(fixture), false);
  assert.deepEqual(fixture.reads, []);
  fixture.github.paginate = async () => {
    throw new Error('GitHub unavailable');
  };
  await assert.rejects(isReleaseMerge(fixture), /GitHub unavailable/);
});
