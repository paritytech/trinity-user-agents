package io.paritytech.polkadotapp.common.domain.model

import java.security.SecureRandom

/**
 * Symmetric key for the protocol AEAD (ChaCha20-Poly1305). Reached either through HKDF over an
 * X25519 shared secret, or straight from a keyed hash in the HOP attachment flow.
 */
@JvmInline
value class AeadKey private constructor(val bytes: DataByteArray) {
    companion object {
        const val SIZE_BYTES = 32

        /**
         * For key material produced by a KDF or hash that already fixes the length. The check is a
         * programming-error assertion, not input validation — use [fromBytes] for anything that
         * came off the wire or out of storage.
         */
        fun fromDerivedBytes(bytes: ByteArray): AeadKey {
            require(bytes.size == SIZE_BYTES) { "Derived AeadKey must be $SIZE_BYTES bytes, got ${bytes.size}" }

            return AeadKey(bytes.toDataByteArray())
        }

        fun random(): AeadKey {
            val bytes = ByteArray(SIZE_BYTES)
                .apply { SecureRandom().nextBytes(this) }.toDataByteArray()
            return AeadKey(bytes)
        }

        fun fromBytes(bytes: DataByteArray): Result<AeadKey> {
            if (bytes.value.size != SIZE_BYTES) {
                return Result.failure(
                    IllegalArgumentException("AeadKey must be $SIZE_BYTES bytes, got ${bytes.value.size}")
                )
            }

            return Result.success(AeadKey(bytes))
        }
    }
}
