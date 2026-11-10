import * as v from 'valibot';

import { createState, persistLocalStorage } from '@/shared/rxstate';

// Recently-opened chats curated by the search panel — a persisted, ordered list
// of session IDs (newest first). Live name/avatar/presence are resolved from the
// session list at render time; only the order is persisted. `removeRecent` / `clearRecent`
// curate this list — they never delete the chat or its messages.
const MAX_RECENT = 12;
const STORAGE_KEY = 'chat_recent_chats_v1';

const recentSchema = v.array(v.string());

const decode = (raw: string): string[] => {
  const parsed: unknown = JSON.parse(raw);
  const result = v.safeParse(recentSchema, parsed);

  return result.success ? result.output : [];
};

const recent$ = createState<string[]>([]);

persistLocalStorage(recent$, {
  key: STORAGE_KEY,
  sync: true,
  decode,
});

const addRecent = (sessionId: string) => {
  recent$.set(prev => [sessionId, ...prev.filter(id => id !== sessionId)].slice(0, MAX_RECENT));
};

const removeRecent = (sessionId: string) => {
  recent$.set(prev => prev.filter(id => id !== sessionId));
};

const clearRecent = () => {
  recent$.set([]);
};

export const recentChats = {
  recent$,
  addRecent,
  removeRecent,
  clearRecent,
};
