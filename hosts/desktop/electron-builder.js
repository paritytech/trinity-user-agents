import {
  appId,
  assertPackagingIdentity,
  author,
  autoUpdateUrl,
  electronProtocol,
  folders,
  title,
  updateFeed,
} from './config/index.js';

// Packaging is the moment an identity starts to mean something to an operating system, so it is
// also the moment a missing one has to stop the build.
assertPackagingIdentity();

const CURRENT_YEAR = new Date().getFullYear();

/**
 * @type {import('electron-builder').Configuration}
 *
 * @see https://www.electron.build/configuration
 */
export default {
  appId: appId,
  productName: title,
  copyright: `Copyright © ${CURRENT_YEAR} — ${author}`,

  directories: {
    app: folders.devBuild,
    output: folders.prodBuild,
  },

  protocols: {
    name: title,
    schemes: [electronProtocol],
  },

  mac: {
    category: 'public.app-category.finance',
    hardenedRuntime: true,
    icon: `${folders.resources}/icons/icon-mac.png`,
    entitlements: `${folders.resources}/entitlements/entitlements.mac.plist`,
    entitlementsInherit: `${folders.resources}/entitlements/entitlements.mac.plist`,
    target: [
      {
        target: 'dmg',
        arch: ['arm64', 'x64'],
      },
      {
        target: 'zip',
        arch: ['arm64', 'x64'],
      },
    ],
    notarize: true,
  },

  linux: {
    icon: `${folders.resources}/icons/icon.png`,
    category: 'Finance',
    target: ['AppImage'],
    mimeTypes: [`x-scheme-handler/${electronProtocol}`],
    desktop: {
      entry: {
        StartupWMClass: title,
      },
    },
  },

  win: {
    icon: `${folders.resources}/icons/icon.ico`,
    target: ['nsis'],
  },

  publish: updateFeed,

  generateUpdatesFilesForAllChannels: false,
  detectUpdateChannel: false,

  compression: 'normal',
  // `${name}` (`polkadot-desktop`), not `${productName}` ("Polkadot Desktop"): a GitHub release
  // asset's name must not contain spaces. GitHub rewrites a space to `.`, while electron-updater
  // rewrites it to `-` when building the download URL from the metadata, so a spaced name is
  // uploaded and requested under two different filenames and every download 404s.
  //
  // A static feed has no such rewriting and its publishing steps address artifacts by the spaced
  // name, so that deployment keeps `${productName}` — changing it would rename every object in the
  // bucket for no benefit.
  artifactName: autoUpdateUrl ? '${productName}-${version}-${arch}.${ext}' : '${name}-${version}-${arch}.${ext}',
};
