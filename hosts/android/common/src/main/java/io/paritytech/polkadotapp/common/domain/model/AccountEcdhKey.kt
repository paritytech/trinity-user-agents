package io.paritytech.polkadotapp.common.domain.model

/**
 * An account's chat encryption key as recorded on chain.
 *
 * RFC-0004 gives the on-chain record a keypair-type marker, so a reader can encounter a key it does
 * not implement. [Unknown] keeps such a value intact instead of failing the surrounding read — the
 * account is still usable for everything that does not need to encrypt to it.
 */
sealed interface AccountEcdhKey {
    data class X25519(val key: X25519PublicKey) : AccountEcdhKey

    /** Retains the full container so an unrecognised key re-encodes byte-for-byte. */
    data class Unknown(val rawValue: DataByteArray) : AccountEcdhKey
}

fun AccountEcdhKey.x25519OrNull(): X25519PublicKey? = (this as? AccountEcdhKey.X25519)?.key
