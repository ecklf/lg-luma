function versionParts(version) {
  if (typeof version !== 'string' || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error(`Invalid numeric release version: ${version}`);
  }
  return version.split('.').map(BigInt);
}

export function isVersionIncrease(previous, current) {
  const before = versionParts(previous);
  const after = versionParts(current);
  for (let index = 0; index < before.length; index++) {
    if (before[index] !== after[index]) return after[index] > before[index];
  }
  return false;
}

export async function isReleaseMerge({ github, context }) {
  if (
    context.eventName !== 'push' ||
    context.ref !== 'refs/heads/main' ||
    context.payload.deleted ||
    context.payload.forced ||
    !context.payload.before ||
    /^0+$/.test(context.payload.before)
  ) {
    return false;
  }

  const repository = `${context.repo.owner}/${context.repo.repo}`;
  const pulls = await github.paginate(github.rest.repos.listPullRequestsAssociatedWithCommit, {
    ...context.repo,
    commit_sha: context.sha,
    per_page: 100,
  });
  const release = pulls.find(
    (pull) =>
      pull.merged_at &&
      pull.merge_commit_sha === context.sha &&
      pull.base.ref === 'main' &&
      pull.base.repo?.full_name === repository &&
      pull.head.ref === 'changeset-release/main' &&
      pull.head.repo?.full_name === repository,
  );
  if (!release) return false;

  const versions = await Promise.all(
    [context.payload.before, context.sha].map(async (ref) => {
      const { data } = await github.rest.repos.getContent({
        ...context.repo,
        ref,
        path: 'package.json',
      });
      if (data.encoding !== 'base64' || typeof data.content !== 'string') {
        throw new Error(`Could not read package.json at ${ref}.`);
      }
      return JSON.parse(Buffer.from(data.content, 'base64').toString('utf8')).version;
    }),
  );
  return isVersionIncrease(...versions);
}
