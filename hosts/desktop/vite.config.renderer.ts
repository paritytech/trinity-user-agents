import { resolve } from 'node:path';

import { type UserConfigFn } from 'vite';

import { folders, renderer, title, version } from './config/index.js';

const config: UserConfigFn = async ({ mode, command }) => {
  const { defineConfig } = await import('vite');

  const { default: svgr } = await import('vite-plugin-svgr');
  const { default: favicons } = await import('@peterek/vite-plugin-favicons');
  const { default: react } = await import('@vitejs/plugin-react');
  const { default: mkcert } = await import('vite-plugin-mkcert');
  const { default: wasm } = await import('vite-plugin-wasm');

  const { tanstackRouter } = await import('@tanstack/router-plugin/vite');

  const { default: tailwindcss } = await import('@tailwindcss/vite');
  const { sentryVitePlugin } = await import('@sentry/vite-plugin');
  const svgrOxcOptions = {
    // Oxc/Rolldown (Vite 8) types this as `'preserve' | { runtime }`; the old
    // string `'react-jsx'` (a TS compilerOptions value) is rejected at build init.
    jsx: { runtime: 'automatic' },
  };

  const isDev = mode === 'development';
  const isStage = mode === 'staging';

  return defineConfig({
    mode: isStage ? 'production' : mode,
    cacheDir: resolve(folders.cache, 'vite-renderer'),
    base: '',
    root: folders.rendererRoot,
    // `.env` files live in the project root; without this `envDir` defaults to
    // `root` (src/) and no `VITE_*` var loads in the renderer.
    envDir: folders.root,
    define: {
      // Original CLI mode (development | staging | production). `mode` below is
      // remapped (staging -> production) for Vite, so import.meta.env.MODE cannot
      // distinguish a staging build from a production one — this define can.
      'process.env.BUILD_MODE': JSON.stringify(mode),
      'process.env.PRODUCT_NAME': JSON.stringify(title),
      'process.env.VERSION': JSON.stringify(version),
      'process.env.BUILD_TIME': JSON.stringify(new Date().toISOString()),
      'process.env.LOGGER': JSON.stringify(process.env['LOGGER']),
      'process.env.SENTRY_DSN': JSON.stringify(process.env['SENTRY_DSN'] ?? ''),
      // AUTOTEST is a runtime value injected via the preload bridge (window.App)
      // Not compile-time — allows reusing the same build for normal and e2e runs
    },
    optimizeDeps: {
      // Injected by babel (react-compiler / automatic JSX runtime) during transform, so vite's
      // esbuild scanner never sees them. Without this the first optimize pass misses them, vite
      // re-runs the optimizer over every dep, and the renderer reloads its whole module graph.
      include: ['react/compiler-runtime', 'react/jsx-runtime'],
      exclude: [
        '@jitl/quickjs-wasmfile-release-asyncify',
        'emscripten-module',
        'quickjs-emscripten',
        '@novasamatech/host-chat',
        '@novasamatech/handoff-service',
      ],
    },
    worker: {
      format: 'es',
      // The TrUAPI core instantiates its wasm inside `@parity/truapi-host/worker-runtime`,
      // and Vite applies plugins to worker bundles only through `worker.plugins` — the
      // top-level `wasm()` below never reaches them.
      plugins: () => [wasm()],
    },
    resolve: {
      tsconfigPaths: true,
      dedupe: ['react', 'react-dom'],
    },
    build: {
      sourcemap: isDev ? undefined : 'hidden',
      chunkSizeWarningLimit: Infinity,
      minify: !isDev,
      outDir: folders.devBuild,
      emptyOutDir: false,
      target: 'es2022',
      rolldownOptions: {
        onLog(level, log, handler) {
          if (log.cause) {
            // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
            const cause = log.cause as Record<string, string>;

            if (cause['message'] === `Can't resolve original location of error.`) {
              return;
            }
          }

          handler(level, log);
        },
      },
    },
    assetsInclude: ['**/*.wasm'],
    server: {
      port: renderer.server.port,
    },
    plugins: [
      tanstackRouter({
        target: 'react',
        routesDirectory: resolve(folders.rendererRoot, 'routes'),
        generatedRouteTree: resolve(folders.rendererRoot, 'routeTree.gen.ts'),
        quoteStyle: 'single',
        autoCodeSplitting: true,
        tmpDir: resolve(folders.tmp, 'tanstack-router'),
      }),

      command === 'serve' && mode !== 'test' && mkcert(),

      wasm(),

      tailwindcss(),

      react({
        // Babel is for builds only. With no babel plugins/presets configured, plugin-react's
        // `canSkipBabel` holds and rolldown-vite deletes its transform hook outright, so dev serve
        // never runs babel at all (~433ms per module over 250 modules — a ~13s cold start).
        // The trade: dev has no react-compiler memoization, so compiler behaviour shows up in builds.
        babel: command === 'serve' ? {} : { compact: false, plugins: ['react-compiler'] },
        exclude: /useTranslation\.ts$/,
      }),
      svgr({
        include: '**/*.svg?jsx',
        // @ts-expect-error upstream typing mismatch in vite-plugin-svgr
        oxcOptions: svgrOxcOptions,
        svgrOptions: {
          plugins: ['@svgr/plugin-svgo', '@svgr/plugin-jsx'],
          memo: true,
          ref: true,
          jsxRuntime: 'automatic',
          svgo: true,
          svgoConfig: {
            plugins: [
              {
                name: 'preset-default',
                params: { overrides: { removeViewBox: false, cleanupIds: false } },
              },
            ],
          },
        },
      }),
      favicons(
        mode === 'development' ? resolve(folders.rendererRoot, 'favicon.dev.png') : resolve(folders.rendererRoot, 'favicon.png'),
        {
          appName: 'Polkadot Desktop',
          appShortName: 'Dot Desktop',
          appDescription: '',
          icons: {
            android: true,
            appleIcon: true,
            appleStartup: true,
            favicons: true,
            windows: true,
            yandex: true,
          },
        },
      ),
      // VitePWA({
      //   devOptions: { enabled: true, type: 'module' },
      //   scope: './',
      //   injectRegister: null,
      //   strategies: 'injectManifest',
      //   srcDir: '.',
      //   filename: 'sw.ts',
      //   injectManifest: {
      //     injectionPoint: undefined,
      //     plugins: commonPlugins,
      //   },
      // }),
      sentryVitePlugin({
        org: process.env['SENTRY_ORG'],
        project: process.env['SENTRY_PROJECT'],
        authToken: process.env['SENTRY_AUTH_TOKEN'],
        release: { name: `polkadot-desktop@${version}` },
        sourcemaps: { filesToDeleteAfterUpload: ['**/*.js.map'] },
        disable: !process.env['SENTRY_AUTH_TOKEN'],
      }),
    ],
  });
};

export default config;
