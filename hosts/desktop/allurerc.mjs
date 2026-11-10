import { defineConfig } from 'allure';

// Allure 3 runtime config consumed by `npx allure generate <allure-results>`.
// `allure-playwright` writes Allure 2 result files into `allure-results/`; the
// Allure 3 CLI reads that format and renders the "awesome" report into `output`.
// The `output` dir below is the directory that `allure-framework/allure-action`
// scans (its `report-directory` input must match this path).
export default defineConfig({
  output: './allure-report',
  plugins: {
    awesome: {
      import: '@allurereport/plugin-awesome',
      options: {
        singleFile: false,
        reportName: 'Polkadot Desktop E2E',
        open: false,
      },
    },
  },
});
