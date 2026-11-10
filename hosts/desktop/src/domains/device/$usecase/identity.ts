import { deviceIdentityRepository } from '../identity/repository';
import { deviceIdentityService } from '../identity/service';
import { type DeviceIdentity } from '../identity/types';

// One in-flight mint per process. Without it two callers arriving in the same tick
// both see an empty store and both generate; the repository would settle the race,
// but only after the loser had already derived from a key it then discards.
let loading: Promise<DeviceIdentity> | null = null;

function toIdentity(statementAccountSeed: Uint8Array, encryptionPrivateKey: Uint8Array): DeviceIdentity {
  return {
    statementAccountSeed,
    statementAccountPublicKey: deviceIdentityService.deriveStatementAccountPublicKey(statementAccountSeed),
    encryptionPrivateKey,
    encryptionPublicKey: deviceIdentityService.deriveEncryptionPublicKey(encryptionPrivateKey),
  };
}

async function load(): Promise<DeviceIdentity> {
  const existing = await deviceIdentityRepository.read();
  if (existing) return toIdentity(existing.statementAccountSeed, existing.encryptionPrivateKey);

  const stored = await deviceIdentityRepository.create({
    id: deviceIdentityRepository.ROW_ID,
    statementAccountSeed: deviceIdentityService.generateStatementAccountSeed(),
    encryptionPrivateKey: deviceIdentityService.generateEncryptionPrivateKey(),
    createdAt: Date.now(),
  });

  return toIdentity(stored.statementAccountSeed, stored.encryptionPrivateKey);
}

/**
 * This device's identity, minted on first use and stable for the install.
 *
 * The host owns these keys outright — they are not derived from the paired session,
 * so they survive a re-pair, and no wallet is involved in creating them. The
 * invariant this enforces is that the install has exactly **one** identity for its
 * lifetime: `statementAccountPublicKey` is how peers address this device in the
 * multi-device protocol, so minting a second one silently orphans every peer that
 * already knows the first.
 */
function getDeviceIdentity(): Promise<DeviceIdentity> {
  loading ??= load().catch((error: unknown) => {
    loading = null;
    throw error;
  });

  return loading;
}

/**
 * Discard this device's identity so the next read mints a fresh one.
 *
 * Cryptographically retires the install as a peer: everything addressed to the old
 * statement account becomes unreachable, which is the point — it runs on logout and
 * on the onboarding retry, where carrying the previous identity forward would let a
 * new user inherit the previous one's peers.
 */
async function resetDeviceIdentity(): Promise<void> {
  loading = null;
  await deviceIdentityRepository.clear();
}

export const deviceIdentityUseCase = {
  getDeviceIdentity,
  resetDeviceIdentity,
};
