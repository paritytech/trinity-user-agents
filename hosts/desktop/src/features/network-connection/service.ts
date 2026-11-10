import { type Chain, type GenesisHash } from '@/domains/network';

// What the active environment uses a network for. Derived from the environment's
// role assignment (`VITE_ENVIRONMENTS.roles` resolved through Remote Config) —
// there is no per-chain field carrying this.
export type ChainRole = 'identity' | 'apps' | 'transactions';

type EnvironmentRoles = {
  peopleChain: Chain;
  bulletinChain: Chain;
  // The `assetHub` role; named after its other job in the environment model.
  dotnsChain: Chain;
};

const rolesForChain = (chainId: GenesisHash, environment: EnvironmentRoles | null): ChainRole[] => {
  if (!environment) return [];

  const roles: ChainRole[] = [];

  if (environment.peopleChain.genesisHash === chainId) roles.push('identity');
  if (environment.dotnsChain.genesisHash === chainId) roles.push('transactions');
  if (environment.bulletinChain.genesisHash === chainId) roles.push('apps');

  return roles;
};

export const networkConnectionService = {
  rolesForChain,
};
