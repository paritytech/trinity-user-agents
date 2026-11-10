import { resolve } from 'path';

import { type UserConfigFn } from 'vite';

import { folders, title, version } from './config/index.js';

// Preload for the dedicated A/V call window. Built as a separate cjs lib
// (call-window-preload.cjs) exactly like the main preload — it only forwards
// the transferred MessagePort + init payload into the call-window main world.
const config: UserConfigFn = async ({ mode }) => {
  const { defineConfig } = await import('vite');
  // @ts-expect-error Broken types
  const { default: target } = await import('vite-plugin-target');

  return defineConfig({
    mode,
    cacheDir: resolve(folders.cache, 'vite-call-window-preload'),
    define: {
      'process.env.PRODUCT_NAME': JSON.stringify(title),
      'process.env.VERSION': JSON.stringify(version),
      'process.env.LOGGER': JSON.stringify(process.env['LOGGER']),
    },
    resolve: {
      tsconfigPaths: true,
    },
    build: {
      outDir: folders.devBuild,
      emptyOutDir: false,
      lib: {
        entry: folders.entrypoint.callWindowPreload,
        fileName: () => 'call-window-preload.cjs',
        formats: ['cjs'],
      },
      rolldownOptions: {
        output: {
          globals: {
            process: 'process',
          },
        },
      },
    },
    plugins: [target({ 'electron-preload': {} })],
  });
};

export default config;
