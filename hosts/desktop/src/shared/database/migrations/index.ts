// Unified migration flow. Each schema bump in `../schema.ts` appends
// `dexie.version(N).stores(<delta>)`, with `.upgrade(migrationN)` when rows move; the
// upgrade function is a named `(tx: Transaction) => Promise<void>` defined in
// `migration-N.ts` and re-exported here. v1 is the baseline schema and has no upgrade.
export { migrateProductStorageToV12 } from './migration-12';
