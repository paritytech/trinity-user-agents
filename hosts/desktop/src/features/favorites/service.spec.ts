import { describe, expect, it } from 'vitest';

import { favoritesService } from './service';

type Item = { title: string };
const items: Item[] = [{ title: 'Coin Flip' }, { title: 'Staking' }, { title: 'coincard' }];

describe('favoritesService.filterByTitle', () => {
  it('returns all items for an empty/whitespace query', () => {
    expect(favoritesService.filterByTitle(items, '', i => i.title)).toEqual(items);
    expect(favoritesService.filterByTitle(items, '   ', i => i.title)).toEqual(items);
  });

  it('matches case-insensitively on a substring', () => {
    expect(favoritesService.filterByTitle(items, 'coin', i => i.title).map(i => i.title)).toEqual(['Coin Flip', 'coincard']);
  });

  it('returns [] when nothing matches', () => {
    expect(favoritesService.filterByTitle(items, 'zzz', i => i.title)).toEqual([]);
  });
});
