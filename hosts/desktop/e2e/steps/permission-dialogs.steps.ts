import { createBdd } from 'playwright-bdd';

import { test } from '../fixtures/test-product-sdk';
import { DashboardPage } from '../page-objects/DashboardPage';

const { When, Then } = createBdd(test);

// Steps for the host-side permission/alias/allocation dialogs driven from
// host-playground. Bound to the test-product-sdk `test` so both `testProductPage`
// (the product webview) and `authenticatedApp` (the host window, where the dialogs
// and theme switch live) are available. Features using the alias/device dialog
// assertions must be tagged @manual-permissions (the auto-approver is off so the
// dialog can be observed); the allocation dialog is never auto-approved, so it
// needs no tag.

// --- Alias permission dialog ------------------------------------------------

Then('the alias permission dialog is shown', async ({ testProductPage }) => {
  await testProductPage.expectAliasPermissionDialog();
});

When('the user approves alias access', async ({ testProductPage }) => {
  await testProductPage.allowAlias();
});

When('the user denies alias access', async ({ testProductPage }) => {
  await testProductPage.denyAlias();
});

Then('the alias permission dialog is dismissed', async ({ testProductPage }) => {
  await testProductPage.expectAliasPermissionDialogClosed();
});

// --- Resource allocation / allowance dialog ---------------------------------

Then('the allowance request dialog is shown', async ({ testProductPage }) => {
  await testProductPage.expectAllocationRequestDialog();
});

// --- Dialog theming ---------------------------------------------------------

When('the user switches to the dark theme', async ({ authenticatedApp }) => {
  await new DashboardPage(authenticatedApp.window).setTheme('Night');
});

Then('the device permission dialog is shown', async ({ testProductPage }) => {
  await testProductPage.expectDevicePermissionDialog();
});

Then('the permission dialog screenshot is taken as {string}', async ({ authenticatedApp, $testInfo }, name: string) => {
  const screenshot = await authenticatedApp.window.screenshot();
  await $testInfo.attach(`${name}-${process.platform}`, { body: screenshot, contentType: 'image/png' });
});
