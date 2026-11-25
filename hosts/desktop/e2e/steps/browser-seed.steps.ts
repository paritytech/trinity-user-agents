import { createBdd } from 'playwright-bdd';

import { TEST_IDS } from '@/shared/test-ids';
import { test } from '../fixtures/link-tests';
import { DEFAULT_ENVIRONMENT_ID, networkTld, productName } from '../helpers/dotns';
import { seedAddressBarRecents, seedProducts } from '../helpers/seed-products';
import { DEFAULT_TIMEOUT } from '../helpers/timeouts';
import { AddressBarPage } from '../page-objects/AddressBarPage';
import { NewTabPage } from '../page-objects/NewTabPage';
import { OnboardingPage } from '../page-objects/OnboardingPage';

const { Given, When, Then } = createBdd(test);

// This project never signs in, but it does not opt out of an environment either
// — it skips the picker, so onboarding's default stays selected and the app
// resolves that network's TLD. Seeded ids have to be the ones it will look for.
const named = (label: string) => productName(label, networkTld(DEFAULT_ENVIRONMENT_ID));

// Two committed products + one of them marked as a recent visit. Drives the
// suggestions' "Recently Opened" section and the "Saved" section below it — a
// product already shown as a recent is not repeated, so this seeds one of each.
Given('seeded products with a recent visit', async ({ electronApp }) => {
  await seedProducts(electronApp.window, [{ baseName: named('coinflipgame03') }, { baseName: named('host-playground') }]);
  await seedAddressBarRecents(electronApp.window, [named('coinflipgame03')]);
});

// The three new-tab pinned labels (`PINNED_LABELS` in NewTab.tsx) plus one
// recent, so the pinned grid renders all three cards and the recent grid one.
Given('seeded pinned and recent products', async ({ electronApp }) => {
  await seedProducts(electronApp.window, [
    { baseName: named('host-playground') },
    { baseName: named('coinflipgame03') },
    { baseName: named('test-dapp-01') },
  ]);
  await seedAddressBarRecents(electronApp.window, [named('coinflipgame03')]);
});

// Recents hydrate from localStorage only at module init (no storage-event
// listener), so a reload is required for seeded recents to appear. A reload
// normally restores the prior `/dashboard` route (no auth guard); only an
// onboarding bounce needs an explicit skip, so skip adaptively.
Given('the app reloads with onboarding skipped', async ({ electronApp }) => {
  const page = electronApp.window;
  await page.reload({ waitUntil: 'domcontentloaded' });
  const onboarding = new OnboardingPage(page);
  const onboardingShown = await page
    .getByTestId(TEST_IDS.onboardingSkip)
    .waitFor({ state: 'visible', timeout: 5_000 })
    .then(() => true)
    .catch(() => false);
  if (onboardingShown) await onboarding.skipOnboarding();
  await page.waitForURL(/dashboard|new-tab/, { timeout: DEFAULT_TIMEOUT });
});

Then('the address bar suggestions show the recents section', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).expectRecentsSectionVisible();
});

Then('the address bar suggestions show the saved section', async ({ electronApp }) => {
  await new AddressBarPage(electronApp.window).expectSavedSectionVisible();
});

Then('the new tab page shows the pinned app grid', async ({ electronApp }) => {
  await new NewTabPage(electronApp.window).expectPinnedCardCount(3);
});

Then('the new tab page shows the recent app grid', async ({ electronApp }) => {
  await new NewTabPage(electronApp.window).expectRecentCardsVisible();
});

When('the user clears recents on the new tab page', async ({ electronApp }) => {
  await new NewTabPage(electronApp.window).clearRecents();
});

Then('the recents-cleared toast is visible', async ({ electronApp }) => {
  await new NewTabPage(electronApp.window).expectUndoToastVisible();
});

Then('the new tab page shows no recent apps', async ({ electronApp }) => {
  await new NewTabPage(electronApp.window).expectNoRecentCards();
});

When('the user undoes clearing recents', async ({ electronApp }) => {
  await new NewTabPage(electronApp.window).undo();
});
