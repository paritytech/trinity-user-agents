import { type AddWidgetContributedEntry, type AddableDashboardCard } from '../../di';

import { type AddWidgetSidebarEntry } from './types';

type TranslateFn = (key: string) => string;

// Comparable/searchable display name: native entries carry an i18n key resolved
// here, contributed entries already hold a concrete (provider-resolved) label.
function sidebarEntryName(entry: AddWidgetSidebarEntry, t: TranslateFn): string {
  return entry.source === 'native' ? t(entry.card.displayNameKey) : entry.entry.label;
}

export function buildNativeSidebarEntries(addableCards: AddableDashboardCard[]): AddWidgetSidebarEntry[] {
  return addableCards.map(card => ({
    source: 'native' as const,
    id: card.gridId,
    card,
  }));
}

export function buildContributedSidebarEntries(entries: AddWidgetContributedEntry[]): AddWidgetSidebarEntry[] {
  return entries.map(entry => ({
    source: 'contributed' as const,
    id: entry.id,
    entry,
  }));
}

export function mergeAddWidgetSidebarEntries(
  nativeEntries: AddWidgetSidebarEntry[],
  contributedEntries: AddWidgetSidebarEntry[],
  t: TranslateFn,
): AddWidgetSidebarEntry[] {
  const sortByName = (a: AddWidgetSidebarEntry, b: AddWidgetSidebarEntry) =>
    sidebarEntryName(a, t).localeCompare(sidebarEntryName(b, t), undefined, { sensitivity: 'base' });

  return [...[...nativeEntries].sort(sortByName), ...[...contributedEntries].sort(sortByName)];
}

export function filterSidebarEntries(entries: AddWidgetSidebarEntry[], query: string, t: TranslateFn): AddWidgetSidebarEntry[] {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) return entries;

  return entries.filter(entry => {
    if (entry.source === 'native') {
      const haystack = `${sidebarEntryName(entry, t)} ${entry.id}`.toLowerCase();
      return haystack.includes(normalizedQuery);
    }

    const { label, id, searchText } = entry.entry;
    const haystack = `${label} ${id} ${searchText ?? ''}`.toLowerCase();
    return haystack.includes(normalizedQuery);
  });
}
