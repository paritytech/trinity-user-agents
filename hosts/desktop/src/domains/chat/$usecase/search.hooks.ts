import { useEffect, useState } from 'react';

import { type MessageSearchHit, searchUseCase } from './search';

// Enough to coalesce a fast typist's keystrokes without the results feeling laggy.
const DEBOUNCE_MS = 200;

/** Debounced full-text message search; a blank query short-circuits without hitting the database. */
export const useMessageSearch = (query: string): { data: MessageSearchHit[]; pending: boolean } => {
  const [data, setData] = useState<MessageSearchHit[]>([]);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    const needle = query.trim();
    if (!needle) {
      setData([]);
      setPending(false);

      return;
    }

    let cancelled = false;
    setPending(true);
    const timer = setTimeout(() => {
      searchUseCase
        .searchMessages(needle)
        .then(results => {
          if (!cancelled) setData(results);
        })
        .catch(() => {
          if (!cancelled) setData([]);
        })
        .finally(() => {
          if (!cancelled) setPending(false);
        });
    }, DEBOUNCE_MS);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query]);

  return { data, pending };
};
