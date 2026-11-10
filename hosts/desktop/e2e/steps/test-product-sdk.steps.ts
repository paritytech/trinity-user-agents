import { createBdd } from 'playwright-bdd';

import { AUTH_TLD } from '../fixtures/authenticated';
import { test } from '../fixtures/test-product-sdk';
import { productName as toProductName } from '../helpers/dotns';

const { Given, When, Then } = createBdd(test);

// Scenarios name the product by its label; the environment decides the suffix.
Given('the test product {string} is opened', async ({ testProductPage }, label: string) => {
  await testProductPage.navigateTo(toProductName(label, AUTH_TLD));
  await testProductPage.waitForProductReady();
});

When('the user clicks the {string} tab', async ({ testProductPage }, tabName: string) => {
  await testProductPage.clickCategory(tabName);
});

When('the user runs {string}', async ({ testProductPage }, actionName: string) => {
  await testProductPage.runAction(actionName);
});

When('the user reloads the product', async ({ testProductPage }) => {
  await testProductPage.reloadProduct();
});

When('the user confirms signing', async ({ testProductPage }) => {
  await testProductPage.confirmSigning();
});

When('the user rejects signing', async ({ testProductPage }) => {
  await testProductPage.rejectSigning();
});

When('the user opens the signing review screen', async ({ testProductPage }) => {
  await testProductPage.waitForSignReviewScreen();
});

Then('the signing review shows account, network and call title', async ({ testProductPage }) => {
  await testProductPage.expectReviewSummaryFields();
});

Then('the signing review details show arguments and call data', async ({ testProductPage }) => {
  await testProductPage.expandReviewDetails();
});

Then('the signing review shows the batch behaviour hint', async ({ testProductPage }) => {
  await testProductPage.expectBatchBehaviorHint();
});

When('the user cancels the signing review', async ({ testProductPage }) => {
  await testProductPage.cancelSignReview();
});

When('the user double-clicks Continue to Sign', async ({ testProductPage }) => {
  await testProductPage.doubleClickContinueToSign();
});

When('the user approves the permission', async ({ testProductPage }) => {
  await testProductPage.approvePermission();
});

When('the user denies the permission', async ({ testProductPage }) => {
  await testProductPage.denyPermission();
});

When('the user dismisses the permission dialog', async ({ testProductPage }) => {
  await testProductPage.dismissPermission();
});

When('the user fires a burst of push notifications', async ({ testProductPage }) => {
  await testProductPage.fireBurstOfPushNotifications();
});

Then('a rate-limit toast names the product {string}', async ({ testProductPage }, label: string) => {
  await testProductPage.expectRateLimitToastNamesProduct(toProductName(label, AUTH_TLD));
});

When('the user schedules push notifications past the limit', async ({ testProductPage }) => {
  await testProductPage.scheduleNotificationsPastLimit();
});

Then('the result contains {string}', async ({ testProductPage, $testInfo }, expectedText: string) => {
  try {
    await testProductPage.expectResultContains(expectedText);
  } catch (err) {
    const diagnostics = await testProductPage.collectWebviewDiagnostics();
    await $testInfo.attach('webview-body-text.txt', {
      body: diagnostics.bodyText,
      contentType: 'text/plain',
    });
    if (diagnostics.screenshot) {
      await $testInfo.attach('webview-screenshot.png', {
        body: diagnostics.screenshot,
        contentType: 'image/png',
      });
    }
    throw err;
  }
});
