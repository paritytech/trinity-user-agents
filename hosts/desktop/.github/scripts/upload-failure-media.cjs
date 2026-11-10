// Uploads each failure-media manifest entry (media/<n>/screenshot.png|video.webm,
// written by render-e2e-report.py --mode=failure-media) as its own workflow
// artifact and writes the artifact's download URL back into the manifest, which
// --mode=pr-comment then reads via --media. Runs inside actions/github-script:
// @actions/artifact needs ACTIONS_RUNTIME_TOKEN, which only JS actions receive —
// a plain `run:` step cannot upload artifacts programmatically.
const fs = require('fs');
const path = require('path');

const RETENTION_DAYS = 14;

function artifactName(entry) {
  const raw = `${entry.project}-${entry.os}-${entry.name}`.toLowerCase();
  const slug = raw
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80);
  // The dir index keeps names unique even when truncation collides.
  return `e2e-failure-${slug}-${entry.dir}`;
}

module.exports = async function uploadFailureMedia({ mediaDir }) {
  const manifestPath = path.join(mediaDir, 'manifest.json');
  if (!fs.existsSync(manifestPath)) {
    console.log('no failure-media manifest — skipping');
    return;
  }
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  if (manifest.length === 0) {
    console.log('failure-media manifest is empty — skipping');
    return;
  }
  // @actions/artifact v6 is ESM-only — dynamic import from CJS.
  const { DefaultArtifactClient } = await import('@actions/artifact');
  const client = new DefaultArtifactClient();
  const { GITHUB_SERVER_URL, GITHUB_REPOSITORY, GITHUB_RUN_ID } = process.env;
  for (const entry of manifest) {
    const dir = path.join(mediaDir, entry.dir);
    const files = entry.files.map((f) => path.join(dir, f));
    try {
      const { id } = await client.uploadArtifact(artifactName(entry), files, dir, {
        retentionDays: RETENTION_DAYS,
      });
      if (!id) throw new Error('upload returned no artifact id');
      entry.url = `${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/actions/runs/${GITHUB_RUN_ID}/artifacts/${id}`;
    } catch (err) {
      // One failed upload drops one link, not the whole step.
      console.warn(`upload failed for ${entry.name}: ${err.message}`);
    }
  }
  fs.writeFileSync(manifestPath, JSON.stringify(manifest));
  console.log(`uploaded ${manifest.filter((e) => e.url).length}/${manifest.length} failure-media artifact(s)`);
};
