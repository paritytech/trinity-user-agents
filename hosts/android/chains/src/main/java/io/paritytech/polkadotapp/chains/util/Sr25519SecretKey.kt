package io.paritytech.polkadotapp.chains.util

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519SubstrateKeypairFactory
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray

/**
 * Expanded sr25519 secret: 32-byte private key concatenated with 32-byte nonce.
 * This is the full secret needed to both sign and soft-derive.
 */
@JvmInline
value class Sr25519SecretKey private constructor(val bytes: DataByteArray) {
    companion object {
        const val SIZE_BYTES = 64

        fun fromKeypair(keypair: Sr25519Keypair): Sr25519SecretKey {
            return Sr25519SecretKey((keypair.privateKey + keypair.nonce).toDataByteArray())
        }

        fun fromBytes(bytes: DataByteArray): Result<Sr25519SecretKey> {
            if (bytes.value.size != SIZE_BYTES) {
                return Result.failure(IllegalArgumentException("Sr25519SecretKey must be $SIZE_BYTES bytes, got ${bytes.value.size}"))
            }

            return Result.success(Sr25519SecretKey(bytes))
        }
    }
}

fun Sr25519SecretKey.deriveKeypair(): Sr25519Keypair = Sr25519SubstrateKeypairFactory.createKeypairFromSecret(bytes.value)
