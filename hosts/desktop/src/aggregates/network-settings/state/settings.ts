import { createState, persistLocalStorage } from '@/shared/rxstate';
import { type EnvironmentId, SETTINGS_STORAGE_KEY, environmentService, environmentUseCase } from '@/domains/application';

type Settings = {
  environmentId: EnvironmentId;
};

const setValue = <T extends keyof Settings>(key: T, value: Settings[T]) => {
  return settings$.set(prev => ({ ...prev, [key]: value }));
};

// The persisted blob is a trust boundary — older builds wrote a different shape
// (0.3.x `{ endpointMode }`) and pre-Remote-Config ids (`paseo-next-v2`) under
// this key. `toEnvironmentId` is the domain's rule for that: anything that isn't
// a known channel resolves to the catalog default, so a legacy value self-heals
// on read instead of being wiped from storage.
const decodeSettings = (raw: string): Settings => {
  const parsed: unknown = JSON.parse(raw);
  const persistedId = typeof parsed === 'object' && parsed !== null && 'environmentId' in parsed ? parsed.environmentId : null;

  return { environmentId: environmentService.toEnvironmentId(persistedId) };
};

const settings$ = createState<Settings>({ environmentId: environmentUseCase.getActiveId() });

persistLocalStorage(settings$, { key: SETTINGS_STORAGE_KEY, decode: decodeSettings });

export const networkSettings = {
  settings$,
  setValue,
};
