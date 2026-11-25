import { describe, expect, it } from 'vitest';

import { formatSectionAndMethod, pathnameMatchesSegment, splitCamelCaseString } from './strings';

describe('shared/lib/onChainUtils/strings', () => {
  describe('pathnameMatchesSegment', () => {
    it('matches exact segment', () => {
      expect(pathnameMatchesSegment('/chat', '/chat')).toBe(true);
    });

    it('matches nested paths', () => {
      expect(pathnameMatchesSegment('/chat/abc', '/chat')).toBe(true);
    });

    it('does not match sibling prefixes', () => {
      expect(pathnameMatchesSegment('/chatroom', '/chat')).toBe(false);
    });
  });

  describe('formatSectionAndMethod', () => {
    it('should make capital and add :', () => {
      expect(formatSectionAndMethod('system', 'remark')).toEqual('System: Remark');
    });

    it('split camel case for method', () => {
      expect(formatSectionAndMethod('proxy', 'addProxy')).toEqual('Proxy: Add proxy');
    });

    it('split camel case for section and method', () => {
      expect(formatSectionAndMethod('simpleProxy', 'addProxy')).toEqual('Simple proxy: Add proxy');
    });

    it('split camel case string into parts divided by space', () => {
      expect(splitCamelCaseString('SudoBalances')).toEqual('Sudo Balances');
    });
  });
});
