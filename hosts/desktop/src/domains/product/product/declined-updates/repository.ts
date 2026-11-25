import { database } from '@/shared/database';
import { type HexString } from '@/shared/types';
import { type ExecutableKind } from '../manifest/constants';

import { type DeclinedUpdate } from './types';

const table = database.declinedUpdates;

function keyOf(baseName: string, kind: ExecutableKind, contenthash: HexString): string {
  return `${baseName}#${kind}#${contenthash}`;
}

async function record(entry: DeclinedUpdate): Promise<void> {
  await table.put({
    key: keyOf(entry.baseName, entry.kind, entry.contenthash),
    baseName: entry.baseName,
    kind: entry.kind,
    contenthash: entry.contenthash,
    version: entry.version,
    declinedAt: Date.now(),
  });
}

async function isDeclined(baseName: string, kind: ExecutableKind, contenthash: HexString): Promise<boolean> {
  const row = await table.get(keyOf(baseName, kind, contenthash));
  return row !== undefined;
}

async function deleteByBaseName(baseName: string): Promise<void> {
  await table.where('baseName').equals(baseName).delete();
}

export const declinedUpdatesRepository = {
  record,
  isDeclined,
  deleteByBaseName,
};
