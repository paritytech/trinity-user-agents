import { type ReactNode } from 'react';

/** Wrap each case-insensitive occurrence of `query` in `text` with a `<mark>`; plain text when blank or no match. */
export function highlightMatch(text: string, query: string): ReactNode {
  const needle = query.trim();
  if (!needle) return text;

  const haystack = text.toLowerCase();
  const lowerNeedle = needle.toLowerCase();
  const parts: { text: string; match: boolean }[] = [];
  let cursor = 0;
  let idx = haystack.indexOf(lowerNeedle, cursor);

  while (idx !== -1) {
    if (idx > cursor) parts.push({ text: text.slice(cursor, idx), match: false });
    parts.push({ text: text.slice(idx, idx + needle.length), match: true });
    cursor = idx + needle.length;
    idx = haystack.indexOf(lowerNeedle, cursor);
  }
  if (cursor < text.length) parts.push({ text: text.slice(cursor), match: false });

  return parts.map((part, i) =>
    part.match ? (
      // eslint-disable-next-line react/no-array-index-key -- positional segments of one static string
      <mark key={i} className="bg-bg-status-warning text-fg-primary">
        {part.text}
      </mark>
    ) : (
      // eslint-disable-next-line react/no-array-index-key -- positional segments of one static string
      <span key={i}>{part.text}</span>
    ),
  );
}
