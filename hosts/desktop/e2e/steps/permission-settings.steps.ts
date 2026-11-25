import { createBdd } from 'playwright-bdd';

import { test } from '../fixtures/test-product-sdk';
import { PermissionSettingsPage } from '../page-objects/PermissionSettingsPage';

const { When, Then } = createBdd(test);

// These steps drive the Settings → Privacy permission entity pages. They bind to
// the test-product-sdk `test` so the same scenario can first grant a permission
// via host-playground (testProductPage) and then assert on the settings pages
// (authenticatedApp.window). Generic settings navigation ("the user opens
// settings", "the user opens the settings page") is reused from settings.steps.

When('the user opens the {string} app from the apps settings list', async ({ authenticatedApp }, query: string) => {
  await new PermissionSettingsPage(authenticatedApp.window).openAppFromList(query);
});

Then('the requested permissions list shows {string}', async ({ authenticatedApp }, label: string) => {
  await new PermissionSettingsPage(authenticatedApp.window).expectRequestedPermission(label);
});

When('the user opens the {string} permission for the app', async ({ authenticatedApp }, label: string) => {
  await new PermissionSettingsPage(authenticatedApp.window).openPermissionEntity(label);
});

When('the user sets the permission status to {string}', async ({ authenticatedApp }, status: string) => {
  await new PermissionSettingsPage(authenticatedApp.window).setStatus(status);
});

When('the user resets the permission to default', async ({ authenticatedApp }) => {
  await new PermissionSettingsPage(authenticatedApp.window).resetToDefault();
});

Then('the permission status is {string}', async ({ authenticatedApp }, status: string) => {
  await new PermissionSettingsPage(authenticatedApp.window).expectStatus(status);
});

Then(
  'the permissions list shows the {string}, {string} and {string} categories',
  async ({ authenticatedApp }, a: string, b: string, c: string) => {
    await new PermissionSettingsPage(authenticatedApp.window).expectCategories([a, b, c]);
  },
);

When('the user opens the {string} permission', async ({ authenticatedApp }, label: string) => {
  await new PermissionSettingsPage(authenticatedApp.window).openPermissionDetail(label);
});

Then('the {string} app is listed for the permission', async ({ authenticatedApp }, query: string) => {
  await new PermissionSettingsPage(authenticatedApp.window).expectAppListed(query);
});

When('the user opens the {string} app from the permission detail', async ({ authenticatedApp }, query: string) => {
  await new PermissionSettingsPage(authenticatedApp.window).openAppFromDetail(query);
});

When('the user opens the alias contexts dialog', async ({ authenticatedApp }) => {
  await new PermissionSettingsPage(authenticatedApp.window).openAliasContexts();
});

Then('the alias contexts dialog lists a granted context', async ({ authenticatedApp }) => {
  await new PermissionSettingsPage(authenticatedApp.window).expectAliasContextsDialog();
});
