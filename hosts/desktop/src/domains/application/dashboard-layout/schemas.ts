import * as v from 'valibot';

import { type DashboardCardPayload } from './types';

// `v.object` ignores unknown entries, so a payload persisted before item order
// became the single placement still parses — its legacy `positions` map is just
// dropped on read.
const folderCardPayloadSchema = v.object({
  kind: v.literal('folder'),
  items: v.array(v.string()),
});

// Opaque content payload: only `kind` is the domain's contract; everything else
// passes through verbatim. The dashboard stores and returns it without reading.
const contentCardPayloadSchema = v.objectWithRest({ kind: v.string() }, v.unknown());

const dashboardCardPayloadSchema = v.union([folderCardPayloadSchema, contentCardPayloadSchema]);

export const parseDashboardCardPayload = (value: unknown): DashboardCardPayload | null => {
  const result = v.safeParse(dashboardCardPayloadSchema, value);
  if (!result.success) return null;
  return result.output;
};
