import { getValue } from 'firebase/remote-config';
import * as v from 'valibot';

import { getRemoteConfigInstance } from './bootstrap';

function readRaw(key: string): string | null {
  const remoteConfig = getRemoteConfigInstance();
  if (!remoteConfig) return null;
  const raw = getValue(remoteConfig, key).asString();
  return raw === '' ? null : raw;
}

function issuesMessage(issues: [v.BaseIssue<unknown>, ...v.BaseIssue<unknown>[]]): string {
  return issues.map(issue => issue.message).join('; ');
}

// Null means absent (RC not ready or the param unset) — the caller decides what
// an absent value means. A param that IS set but unparseable or schema-invalid is
// a deployed config bug, not an absence: throw with the reason (the schema is the
// trust boundary; RC is untrusted) so it surfaces instead of silently falling back.
// Reads are sync against the last-activated snapshot, so callers that need fresh
// data `await remoteConfigReady` first.
function tryGetJson<TSchema extends v.GenericSchema>(key: string, schema: TSchema): v.InferOutput<TSchema> | null {
  const raw = readRaw(key);
  if (raw === null) return null;

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new Error(`[remote-config] param "${key}" is not valid JSON`, { cause: error });
  }

  const result = v.safeParse(schema, parsed);
  if (!result.success) {
    throw new Error(`[remote-config] param "${key}" failed validation: ${issuesMessage(result.issues)}`, {
      cause: result.issues,
    });
  }
  return result.output;
}

function tryGetString<TSchema extends v.GenericSchema<string>>(key: string, schema: TSchema): v.InferOutput<TSchema> | null {
  const raw = readRaw(key);
  if (raw === null) return null;

  const result = v.safeParse(schema, raw);
  if (!result.success) {
    throw new Error(`[remote-config] param "${key}" failed validation: ${issuesMessage(result.issues)}`, {
      cause: result.issues,
    });
  }
  return result.output;
}

export const remoteConfigGateway = {
  tryGetJson,
  tryGetString,
};
