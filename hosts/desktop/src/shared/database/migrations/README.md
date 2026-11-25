# Database migrations

One unified migration flow for `polkadot-desktop-app-v1`.

## Adding a migration

1. Add `migration-<N>.ts` exporting one named `(tx: Transaction) => Promise<void>`:

   ```ts
   import { type Transaction } from 'dexie';

   export async function migrateThing(tx: Transaction): Promise<void> {
     await tx
       .table('things')
       .toCollection()
       .modify(row => {
         // mutate row in place; keep idempotent
       });
   }
   ```

2. Re-export it from `index.ts`.
3. In `../schema.ts`, append `dexie.version(<N>).stores(<delta>)` — only the stores that
   change, `null` deleting one — with `.upgrade(migrateThing)` when rows move.

Keep migrations idempotent. Index-only changes need no upgrade callback.
