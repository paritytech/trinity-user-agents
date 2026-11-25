import * as v from 'valibot';

// Resources.StatementStoreAllowances value (People chain). Only `account_id`
// matters; extra fields (seq, since, …) are ignored and never break parsing.
export const statementStoreSlotValueSchema = v.object({
  account_id: v.string(),
});
