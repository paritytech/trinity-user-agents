/**
 * The Statements view's one read on demand: {@link readGrants} reads
 * `:statement_allowance:` for accounts at the newest finalized People block,
 * what the runtime grants each one, not what it uses.
 *
 * The inspector opens no statement subscription of its own: a subscription
 * made to look would take the core's chat topics off the light client's
 * shared filter while it ran, and perhaps after
 * (`src/platform/allowance-finality.ts` says how a grant read avoids that).
 */
export {
  decodeGrant,
  readGrants,
} from "../../src/platform/allowance-finality.js";
export type {
  Grant,
  PeopleAccess,
} from "../../src/platform/allowance-finality.js";
