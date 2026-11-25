import { type ReactNode } from 'react';

// Wraps every case-insensitive occurrence of `query` in `text` with a <mark>.
// Shared by the in-room search results and the sidebar search "Messages" group.
export const renderHighlighted = (text: string, query: string): ReactNode => {
  const trimmed = query.trim();
  if (!trimmed) return text;
  const lower = text.toLowerCase();
  const q = trimmed.toLowerCase();
  const parts: { text: string; match: boolean }[] = [];
  let cursor = 0;
  let idx = lower.indexOf(q, cursor);
  while (idx !== -1) {
    if (idx > cursor) parts.push({ text: text.slice(cursor, idx), match: false });
    parts.push({ text: text.slice(idx, idx + q.length), match: true });
    cursor = idx + q.length;
    idx = lower.indexOf(q, cursor);
  }
  if (cursor < text.length) parts.push({ text: text.slice(cursor), match: false });
  return parts.map((part, i) =>
    part.match ? (
      // eslint-disable-next-line react/no-array-index-key
      <mark key={i} className="bg-bg-status-warning text-fg-primary">
        {part.text}
      </mark>
    ) : (
      // eslint-disable-next-line react/no-array-index-key
      <span key={i}>{part.text}</span>
    ),
  );
};
