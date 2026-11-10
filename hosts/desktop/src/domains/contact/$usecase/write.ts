/**
 * Contact write commands — the chokepoint for mutating the local contact list.
 *
 * These compose contact persistence with the device-sync "local change" signal
 * so a local mutation immediately pokes the sync pump (instead of waiting for
 * the collector's next poll). Keeping the emit here — not in `repository.ts` —
 * is why the repository stays a pure persistence leaf: emitting a cross-domain
 * signal is orchestration, which belongs in a use case.
 *
 * Signal policy (unchanged from the old repository behaviour):
 *   upsert / delete       → local change, signal sync.
 *   applyRemoteContactDelete → inbound sync removal, MUST NOT signal (no echo).
 */

import { signalLocalChange } from '@/domains/device-sync';
import { contactRepository } from '../identity/repository';
import { type Contact } from '../identity/types';

const upsertContact = async (contact: Omit<Contact, 'lastUpdate'> & Partial<Pick<Contact, 'lastUpdate'>>): Promise<void> => {
  await contactRepository.upsert(contact);
  signalLocalChange();
};

const deleteContact = async (accountId: string): Promise<void> => {
  await contactRepository.delete(accountId);
  signalLocalChange();
};

/** Inbound sync removal: drop WITHOUT signalling, so we don't echo the deletion back to peers. */
const applyRemoteContactDelete = async (accountId: string): Promise<void> => {
  await contactRepository.applyRemoteDelete(accountId);
};

export const contactWriteUseCase = {
  upsertContact,
  deleteContact,
  applyRemoteContactDelete,
};
