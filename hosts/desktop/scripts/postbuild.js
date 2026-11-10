import { readFileSync } from 'node:fs';
import { writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

import { appName, author, folders } from '../config/index.js';

const packageJSON = JSON.parse(readFileSync('./package.json', { encoding: 'utf-8' }));

async function createPackageJSONDistVersion() {
  // eslint-disable-next-line no-unused-vars
  const {
    main,
    scripts: _1,
    dependencies: _2,
    devDependencies: _3,
    sideEffects: _4,
    engines: _5,
    ...restOfPackageJSON
  } = packageJSON;

  const entry = main?.split('/')?.reverse()?.[0];
  const packageJSONDistVersion = {
    main: entry || 'main.js',
    ...restOfPackageJSON,
  };

  // Electron derives the user-data directory from the packaged `name`, so it belongs to the same
  // configured identity as the bundle id — two applications sharing one profile would have a
  // develop build reading a release build's accounts and sessions. Unconfigured it stays the
  // package name, which is enough for the unpackaged builds this also runs for; packaging without
  // `APP_NAME` is refused in config/index.js.
  packageJSONDistVersion.name = appName;

  // electron-builder reads the packaged manifest for metadata it does not get from
  // `electron-builder.js` — the Linux maintainer among it — so the configured author has to land
  // here too, or the copyright line and the package metadata would name two different parties.
  packageJSONDistVersion.author = { ...packageJSON.author, name: author };

  try {
    await writeFile(resolve(folders.devBuild, 'package.json'), JSON.stringify(packageJSONDistVersion, null, 2));
  } catch ({ message }) {
    console.log(`
    🛑 Something went wrong!\n
      🧐 There was a problem creating the package.json dist version...\n
      👀 Error: ${message}
    `);
  }
}

createPackageJSONDistVersion();
