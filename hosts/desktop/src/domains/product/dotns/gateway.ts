import { toHex } from '@novasamatech/scale';
import { Binary } from 'polkadot-api';
import * as v from 'valibot';
import { type Hex, decodeFunctionResult, encodeFunctionData } from 'viem';

import { type HexString } from '@/shared/types';
import { hasProperty } from '@/shared/utils';
import { type Environment } from '@/domains/application';
import { chainRegistry } from '@/domains/network';

import {
  DOTNS_CONTENT_RESOLVER_ABI,
  DOTNS_PROTOCOL_REGISTRY_ABI,
  DOTNS_REGISTRY_ABI,
  DRY_RUN_STORAGE_LIMIT,
  DRY_RUN_WEIGHT_LIMIT,
  ZERO_ADDRESS,
} from './constants';
import { networkTldSchema } from './schemas';
import { dotNsService } from './service';

// Stateless dotNS contract reads, dispatched as `ReviveApi.call` dry-runs on
// the active environment's dotNS chain. The only wire-level boundary the
// product domain crosses to reach dotNS — both the legacy contenthash path
// and the manifest-based resolution path go through this gateway. The active
// `env` is passed in by the caller (a use case) — this leaf resolves nothing.

function isZeroAddress(address: string): boolean {
  return address.toLowerCase() === ZERO_ADDRESS;
}

// Null on runtime failure (revert, OOG) so callers can map to the appropriate
// "absent" semantics.
async function reviveCall(env: Environment, contractAddress: string, calldata: Hex): Promise<Hex | null> {
  return chainRegistry.requestApi(env.dotnsChain, async ({ api }) => {
    if (!hasProperty(api.apis, 'ReviveApi')) {
      throw new Error('ReviveApi not found on dotNS chain.');
    }

    const result = await api.apis.ReviveApi.call(
      dotNsService.reviveOriginAccount(),
      contractAddress,
      0n,
      DRY_RUN_WEIGHT_LIMIT,
      DRY_RUN_STORAGE_LIMIT,
      Binary.fromHex(calldata),
    );

    if (!result.result.success) return null;

    return toHex(result.result.value.data);
  });
}

// Null when the registry holds no record (sentinel `address(0)`).
async function readResolver(env: Environment, node: HexString): Promise<HexString | null> {
  const calldata = encodeFunctionData({
    abi: DOTNS_REGISTRY_ABI,
    functionName: 'resolver',
    args: [node],
  });

  const raw = await reviveCall(env, env.dotnsRegistryContract, calldata);
  if (!raw) return null;

  const address = decodeFunctionResult({
    abi: DOTNS_REGISTRY_ABI,
    functionName: 'resolver',
    data: raw,
  });

  return isZeroAddress(address) ? null : address;
}

async function readOwner(env: Environment, node: HexString): Promise<HexString | null> {
  const calldata = encodeFunctionData({
    abi: DOTNS_REGISTRY_ABI,
    functionName: 'owner',
    args: [node],
  });

  const raw = await reviveCall(env, env.dotnsRegistryContract, calldata);
  if (!raw) return null;

  const address = decodeFunctionResult({
    abi: DOTNS_REGISTRY_ABI,
    functionName: 'owner',
    data: raw,
  });

  return isZeroAddress(address) ? null : address;
}

/**
 * The network's dotNS TLD, from `DotnsProtocolRegistry.tld()`. The protocol
 * registry is reached through the registry contract's public `protocolRegistry`
 * field, so no extra address is configured. Null when the deployment predates
 * the per-network TLD (those registries answer with empty data) or reports a
 * value that is not a DNS label.
 */
async function readTld(env: Environment): Promise<string | null> {
  const registryCalldata = encodeFunctionData({ abi: DOTNS_REGISTRY_ABI, functionName: 'protocolRegistry' });

  const registryRaw = await reviveCall(env, env.dotnsRegistryContract, registryCalldata);
  if (!registryRaw || registryRaw === '0x') return null;

  const protocolRegistry = decodeFunctionResult({
    abi: DOTNS_REGISTRY_ABI,
    functionName: 'protocolRegistry',
    data: registryRaw,
  });

  const tldCalldata = encodeFunctionData({ abi: DOTNS_PROTOCOL_REGISTRY_ABI, functionName: 'tld' });

  const tldRaw = await reviveCall(env, protocolRegistry, tldCalldata);
  if (!tldRaw || tldRaw === '0x') return null;

  const tld = decodeFunctionResult({ abi: DOTNS_PROTOCOL_REGISTRY_ABI, functionName: 'tld', data: tldRaw });

  return v.safeParse(networkTldSchema, tld).success ? tld : null;
}

// Caller passes the resolver previously discovered via `readResolver(env, node)`
// — the resolver address is never assumed. Empty string is the dotNS
// sentinel for "no such text record".
async function readText(env: Environment, resolverAddress: HexString, node: HexString, key: string): Promise<string | null> {
  const calldata = encodeFunctionData({
    abi: DOTNS_CONTENT_RESOLVER_ABI,
    functionName: 'text',
    args: [node, key],
  });

  const raw = await reviveCall(env, resolverAddress, calldata);
  if (!raw || raw === '0x') return null;

  return decodeFunctionResult({
    abi: DOTNS_CONTENT_RESOLVER_ABI,
    functionName: 'text',
    data: raw,
  });
}

// Caller passes the resolver previously discovered via `readResolver(env, node)`
// — the resolver address is never assumed. `'0x'` is the dotNS
// sentinel for "no contenthash set" — collapsed here to null.
async function readContentHashAt(env: Environment, resolverAddress: HexString, node: HexString): Promise<HexString | null> {
  const calldata = encodeFunctionData({
    abi: DOTNS_CONTENT_RESOLVER_ABI,
    functionName: 'contenthash',
    args: [node],
  });

  const raw = await reviveCall(env, resolverAddress, calldata);
  if (!raw || raw === '0x') return null;

  const contenthash = decodeFunctionResult({
    abi: DOTNS_CONTENT_RESOLVER_ABI,
    functionName: 'contenthash',
    data: raw,
  });

  return contenthash === '0x' ? null : contenthash;
}

// Legacy (pre-manifest / EIP-1577) contenthash, read straight from the
// environment's global content-resolver contract — no registry → per-name
// resolver indirection. Legacy products have no registry resolver entry; their
// contenthash lives on this one fixed contract. This is how the whole app
// resolved products before manifests existed.
async function readLegacyContentHash(env: Environment, node: HexString): Promise<HexString | null> {
  return readContentHashAt(env, env.dotnsContentResolverContract, node);
}

export const dotNsGateway = {
  readResolver,
  readOwner,
  readTld,
  readText,
  readContentHashAt,
  readLegacyContentHash,
};
