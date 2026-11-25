/** How many recently-opened products are remembered. */
export const MAX_RECENT_PRODUCTS = 20;

// The e2e seeding helper writes this key too (`e2e/helpers/seed-products.ts`),
// prefixed the way `persistLocalStorage` prefixes it — it imports the constant
// rather than restating the string, so a rename here reaches it.
export const RECENT_PRODUCTS_STORAGE_KEY = 'recents/v1';
