import { defineConfig } from 'allure';

// Variant of allurerc.mjs that renders the report as ONE self-contained
// index.html (singleFile: true), used by the e2e `report` job to publish a
// downloadable artifact. The default allurerc.mjs stays multi-file because
// allure-framework/allure-action needs the unpacked data structure to scan.
export default defineConfig({
  output: './allure-report-single',
  plugins: {
    awesome: {
      import: '@allurereport/plugin-awesome',
      options: {
        singleFile: true,
        reportName: 'Polkadot Desktop E2E',
        open: false,
      },
    },
  },
});
