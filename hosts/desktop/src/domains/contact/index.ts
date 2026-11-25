export type { Contact, Device } from './identity/types';
export { contactService } from './identity/service';
// DEBT (largest of the five): read directly by `chat/p2p`, `application`, and `device-sync`.
// Fix: roster/write use cases already exist — route those consumers through them.
// eslint-disable-next-line local-rules/enforce-import-restrictions
export { contactRepository } from './identity/repository';
export { DeviceAdded, DeviceRemoved, DeviceRosterEvent } from './identity/schemas';
export { contactWriteUseCase } from './$usecase/write';
export { rosterUseCase } from './$usecase/roster';
export type { RosterSubscriberDeps } from './$usecase/roster';
