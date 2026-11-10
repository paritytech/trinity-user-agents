import { resolve } from 'node:path';

import packageJson from '../package.json' with { type: 'json' };

const { author: AUTHOR, description: DESCRIPTION, name: NAME, version: VERSION } = packageJson;

export const name = NAME;
export const version = VERSION;
export const description = DESCRIPTION;
export const electronProtocol = 'polkadot';

// The identity a packaged build ships under — bundle id, display name, and the packaged `name`
// Electron derives the user-data directory from — is deployment configuration, not something this
// repository names. There is deliberately no default: a bundle id is what an operating system
// matches an update against, so a build that guessed one would install beside the application it
// was meant to replace, and it would do that silently. `assertPackagingIdentity` below turns that
// into a failure at packaging time instead.
//
// `BUILD_TYPE` selects between two of them. A deployment that wants develop and production to be
// separate applications — separate install slot, profile and signing identity, so a dev build
// cannot overwrite a release one — configures the `_DEVELOP` variants. One that needs a single
// identity, because its installed users already have one, configures only the base variables and
// gets it for every build type.
const isProductionRelease = process.env.BUILD_TYPE === 'production';

function pick(base, develop) {
  const baseValue = (process.env[base] ?? '').trim();
  if (isProductionRelease) return baseValue;

  return (process.env[develop] ?? '').trim() || baseValue;
}

const configuredTitle = pick('PRODUCT_NAME', 'PRODUCT_NAME_DEVELOP');

// Bundles built without an identity — local development, and the fork pull requests that build and
// run e2e with no repository variables at all — still need a display name: it reaches the renderer
// as PRODUCT_NAME and keys the persisted storage the suite asserts against. Derived from the
// package name so it stays a placeholder rather than a second identity living in the tree.
const placeholderTitle = NAME.split('-')
  .map(word => word.charAt(0).toUpperCase() + word.slice(1))
  .join(' ');

export const appId = pick('APP_ID', 'APP_ID_DEVELOP');
export const title = configuredTitle || placeholderTitle;
export const appName = pick('APP_NAME', 'APP_NAME_DEVELOP') || NAME;

// Who the packaged build is copyrighted to, and the maintainer electron-builder records in the
// Linux package metadata. Configuration for the same reason the identity above is: a fork ships
// under its own name, and that is not something this repository can name for it.
//
// Unlike the identity it keeps a default, and it is deliberately absent from
// `assertPackagingIdentity`. The no-default rule exists because a guessed bundle id silently
// installs beside the application it should have replaced; a copyright line cannot break anything
// it is wrong about. Requiring it would cost a fork a failed `npm run dist` and buy nothing.
//
// No `_DEVELOP` variant either — authorship does not differ between a develop and a release build.
export const author = (process.env.APP_AUTHOR ?? '').trim() || AUTHOR.name;

/**
 * Refuse to package without a configured identity. Called from `electron-builder.js`, which runs
 * only for `npm run dist` — building the bundles is left alone so a fork can still build and test
 * without configuring anything.
 */
export function assertPackagingIdentity() {
  const missing = [
    ['APP_ID', appId],
    ['PRODUCT_NAME', configuredTitle],
    ['APP_NAME', pick('APP_NAME', 'APP_NAME_DEVELOP')],
  ]
    .filter(([, value]) => !value)
    .map(([variable]) => (isProductionRelease ? variable : `${variable} (or ${variable}_DEVELOP)`));

  if (missing.length > 0) {
    throw new Error(
      `Cannot package without an application identity. Missing: ${missing.join(', ')}. ` + `See docs/PUBLISHING.md section 4.`,
    );
  }
}

export const main = {
  window: {
    width: 800,
    defaultWidth: 1372,
    height: 800,
    defaultHeight: 800,
  },
};

const rendererUrl = new URL('https://localhost:4000');

export const renderer = {
  server: {
    origin: rendererUrl.origin,
    protocol: rendererUrl.protocol,
    host: rendererUrl.hostname,
    port: parseInt(rendererUrl.port),
  },
};

// Where the auto-updater reads releases from, and what electron-builder generates its update
// metadata for.
//
// GitHub Releases is the default and needs no token for a public repository. `AUTO_UPDATE_URL`
// switches the build to a plain static feed instead — a pipeline publishing to object storage, or
// one whose releases live in a private repository, where the GitHub provider would need a token
// the installed app cannot hold. Setting it wins, because a build that has to reach an existing
// static feed cannot fall back to a repository its users never installed from.
export const autoUpdateUrl = (process.env.AUTO_UPDATE_URL || '').trim();

export const updateRepo = {
  owner: process.env.UPDATE_REPO_OWNER || 'paritytech',
  name: process.env.UPDATE_REPO_NAME || 'polkadot-desktop-community',
};

export const updateFeed = autoUpdateUrl
  ? { provider: 'generic', url: autoUpdateUrl }
  : { provider: 'github', owner: updateRepo.owner, repo: updateRepo.name };

export const folders = {
  entrypoint: {
    main: resolve('./main/index.ts'),
    preload: resolve('./main/preload.ts'),
    renderer: resolve('./src/index.html'),
    // Dedicated A/V call window preload cjs lib. The call UI itself is the
    // `/call` route in the main renderer bundle (not a separate page); this
    // preload delivers the launch params + MessagePort into that window.
    callWindowPreload: resolve('./main/call-window-preload.ts'),
  },

  root: resolve('./'),
  mainRoot: resolve('./main'),
  rendererRoot: resolve('./src'),
  resources: resolve('./main/resources'),
  docs: resolve('./docs'),

  devBuild: resolve('./release/build'),
  prodBuild: resolve('./release/dist'),
  storybookBuild: resolve('./release/storybook'),
  docsBuild: resolve('./release/docs'),

  coverage: resolve('./.coverage'),
  cache: resolve('./node_modules/.cache'),
  tmp: resolve('./node_modules/.tmp'),
};
