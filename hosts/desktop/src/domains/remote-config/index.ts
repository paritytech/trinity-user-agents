export { REMOTE_CONFIG_KEYS } from './constants';
export { type FirebaseConfig, remoteUrlSchema } from './schemas';
export { bootstrapRemoteConfig, refreshRemoteConfig, remoteConfigReady } from './bootstrap';
// DEBT: read by `network/chain/resource.ts` and `application/environment/resource.ts` —
// a resource in another domain reaching a gateway, the exact shape this rule exists for.
// Fix: the value becomes a parameter those resources receive (project-structure.md).
// eslint-disable-next-line local-rules/enforce-import-restrictions
export { remoteConfigGateway } from './gateway';
