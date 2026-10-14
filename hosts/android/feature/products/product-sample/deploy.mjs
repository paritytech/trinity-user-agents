#!/usr/bin/env node
/**
 * Builds the sample product and publishes the bundle as a GitHub gist. stdout is exactly the URL to
 * paste into the app's debug menu; all progress goes to stderr.
 *
 * The gist file is named after the current git branch, so each branch owns one deploy. Any earlier
 * gist carrying that same file name is deleted, leaving exactly one gist per branch.
 *
 * Usage:
 *   node deploy.mjs                  build and publish as <branch>.js
 *   node deploy.mjs --minify         publish the minified bundle (~2x smaller)
 *   node deploy.mjs --skip-build     publish whatever is already in scripts/
 *   node deploy.mjs --public         create a public gist
 *
 * Minification is opt-in: it currently mangles method names the product relies on.
 */

import { spawnSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectDir = dirname(fileURLToPath(import.meta.url));
const BUNDLE_PATH = join(projectDir, 'scripts', 'products_demo.js');

function fail(message) {
    console.error(`\n✗ ${message}\n`);
    process.exit(1);
}

function parseArgs(argv) {
    const args = { minify: false, build: true, public: false };

    for (const arg of argv) {
        switch (arg) {
            case '--minify':
                args.minify = true;
                break;
            case '--skip-build':
                args.build = false;
                break;
            case '--public':
                args.public = true;
                break;
            default:
                fail(`Unknown argument: ${arg}`);
        }
    }

    return args;
}

function run(command, commandArgs, options = {}) {
    const result = spawnSync(command, commandArgs, { cwd: projectDir, encoding: 'utf8', ...options });

    if (result.error) fail(`Failed to run ${command}: ${result.error.message}`);
    if (result.status !== 0) fail(`${command} exited with ${result.status}\n${result.stderr ?? ''}`);

    return result.stdout;
}

function requireGhAuth() {
    const result = spawnSync('gh', ['auth', 'status'], { encoding: 'utf8' });

    if (result.error) fail('GitHub CLI (gh) not found — install it from https://cli.github.com');
    if (result.status !== 0) fail(`gh is not authenticated. Run: gh auth login\n${result.stderr ?? ''}`);
}

/** `feature/rfc0023` -> `feature-rfc0023.js`; gist file names cannot carry path separators. */
function scriptNameFromBranch() {
    const branch = run('git', ['rev-parse', '--abbrev-ref', 'HEAD']).trim();

    if (!branch || branch === 'HEAD') fail('Detached HEAD — check out a branch to name the deploy.');

    const sanitized = branch.replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '');
    if (!sanitized) fail(`Branch "${branch}" does not yield a usable file name.`);

    return `${sanitized}.js`;
}

function build(minify) {
    const script = minify ? 'build:prod:minified' : 'build:prod';
    console.error(`→ npm run ${script}`);
    // esbuild reports to stdout; redirect it to fd 2 so stdout carries only the URL.
    run('npm', ['run', script], { stdio: ['ignore', 2, 'inherit'] });
}

function findGistsNamed(scriptName) {
    const stdout = run('gh', [
        'api', 'gists', '--paginate',
        '--jq', `.[] | select(.files | has("${scriptName}")) | .id`,
    ]);

    return stdout.split('\n').map(line => line.trim()).filter(Boolean);
}

function createGist(scriptName, content, isPublic) {
    const payload = {
        description: `product-sample — ${scriptName} — built ${new Date().toISOString()}`,
        public: isPublic,
        files: { [scriptName]: { content } },
    };

    console.error('→ POST gists');
    const stdout = run('gh', ['api', 'gists', '--method', 'POST', '--input', '-'], {
        input: JSON.stringify(payload),
        stdio: ['pipe', 'pipe', 'pipe'],
    });

    return JSON.parse(stdout);
}

function deleteGist(id) {
    console.error(`→ DELETE gists/${id}`);
    run('gh', ['api', `gists/${id}`, '--method', 'DELETE'], { stdio: ['ignore', 'pipe', 'pipe'] });
}

const args = parseArgs(process.argv.slice(2));

requireGhAuth();

const scriptName = scriptNameFromBranch();
console.error(`→ script name ${scriptName}`);

if (args.build) build(args.minify);

if (!existsSync(BUNDLE_PATH)) fail(`Bundle not found at ${BUNDLE_PATH}. Drop --skip-build.`);

const content = readFileSync(BUNDLE_PATH, 'utf8');
console.error(`→ ${(Buffer.byteLength(content) / 1024 / 1024).toFixed(2)} MB`);

// Publish before removing the previous gist, so a failed upload never leaves the branch undeployed.
const staleGistIds = findGistsNamed(scriptName);
const gist = createGist(scriptName, content, args.public);
staleGistIds.forEach(deleteGist);

// githack, not the gist raw host: the latter serves text/plain, which the WebView rejects under
// strict MIME checking for module scripts. This is githack's uncached endpoint, so a redeploy is
// picked up immediately.
console.log(`https://gist.githack.com/${gist.owner.login}/${gist.id}/raw/${scriptName}`);
