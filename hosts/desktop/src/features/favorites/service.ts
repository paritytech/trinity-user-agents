// Stateless search rule for the Favorites SPA and the Add-to-Favorites dialog:
// a case-insensitive substring match over a caller-supplied title. Single source
// of the filter so the SPA and the dialog behave identically.
function filterByTitle<T>(items: T[], query: string, getTitle: (item: T) => string): T[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return items;

  return items.filter(item => getTitle(item).toLowerCase().includes(normalized));
}

export const favoritesService = { filterByTitle };
