// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest';

import { recentChats } from './recentChats';

const read = () => recentChats.recent$.get();

describe('recentChats', () => {
  it('adds newest-first and dedupes', () => {
    recentChats.addRecent('a');
    recentChats.addRecent('b');
    recentChats.addRecent('a');
    expect(read()).toEqual(['a', 'b']);
  });

  it('removes one id', () => {
    recentChats.addRecent('a');
    recentChats.addRecent('b');
    recentChats.removeRecent('a');
    expect(read()).toEqual(['b']);
  });

  it('clears all', () => {
    recentChats.addRecent('a');
    recentChats.clearRecent();
    expect(read()).toEqual([]);
  });

  it('caps the list length at 12, keeping the newest', () => {
    for (const id of ['1', '2', '3', '4', '5', '6', '7', '8', '9', '10', '11', '12', '13']) {
      recentChats.addRecent(id);
    }
    expect(read().length).toBe(12);
    expect(read()[0]).toBe('13');
    expect(read()).not.toContain('1');
  });
});
