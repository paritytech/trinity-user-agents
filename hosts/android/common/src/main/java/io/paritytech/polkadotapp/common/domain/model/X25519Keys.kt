package io.paritytech.polkadotapp.common.domain.model

private const val X25519_KEY_SIZE_BYTES = 32

/**
 * X25519 public key (RFC 7748): the 32-byte little-endian u-coordinate.
 *
 * Any 32-byte string is a valid public key — there is no on-curve check to perform — so length is
 * the only thing to validate. Small-order points are rejected on the agreement *output* instead.
 */
@JvmInline
value class X25519PublicKey private constructor(val bytes: DataByteArray) {
    companion object {
        const val SIZE_BYTES = X25519_KEY_SIZE_BYTES

        internal fun fromValidated(bytes: ByteArray): X25519PublicKey = X25519PublicKey(bytes.toDataByteArray())

        fun fromBytes(bytes: DataByteArray): Result<X25519PublicKey> {
            return sizeChecked(bytes, "X25519PublicKey").map(::X25519PublicKey)
        }
    }
}

@JvmInline
value class X25519PrivateKey private constructor(val bytes: DataByteArray) {
    companion object {
        const val SIZE_BYTES = X25519_KEY_SIZE_BYTES

        internal fun fromValidated(bytes: ByteArray): X25519PrivateKey = X25519PrivateKey(bytes.toDataByteArray())

        /**
         * For key material produced by a KDF that already fixes the length. The check is a
         * programming-error assertion, not input validation — use [fromBytes] for anything
         * that came off the wire or out of storage.
         */
        fun fromDerivedBytes(bytes: ByteArray): X25519PrivateKey {
            require(bytes.size == SIZE_BYTES) { "Derived X25519PrivateKey must be $SIZE_BYTES bytes, got ${bytes.size}" }

            return X25519PrivateKey(bytes.toDataByteArray())
        }

        fun fromBytes(bytes: DataByteArray): Result<X25519PrivateKey> {
            return sizeChecked(bytes, "X25519PrivateKey").map(::X25519PrivateKey)
        }
    }
}

data class X25519KeyPair(val privateKey: X25519PrivateKey, val publicKey: X25519PublicKey)

/**
 * Raw X25519 agreement output, before key derivation. Distinct from
 * [io.paritytech.polkadotapp.common.domain.model.AeadKey] so the pre- and post-HKDF values cannot be
 * mistaken for one another.
 */
@JvmInline
value class X25519SharedSecret private constructor(val bytes: DataByteArray) {
    companion object {
        const val SIZE_BYTES = X25519_KEY_SIZE_BYTES

        internal fun fromValidated(bytes: ByteArray): X25519SharedSecret = X25519SharedSecret(bytes.toDataByteArray())

        fun fromBytes(bytes: DataByteArray): Result<X25519SharedSecret> {
            return sizeChecked(bytes, "X25519SharedSecret").map(::X25519SharedSecret)
        }
    }
}

private fun sizeChecked(bytes: DataByteArray, typeName: String): Result<DataByteArray> {
    if (bytes.value.size != X25519_KEY_SIZE_BYTES) {
        return Result.failure(
            IllegalArgumentException("$typeName must be $X25519_KEY_SIZE_BYTES bytes, got ${bytes.value.size}")
        )
    }

    return Result.success(bytes)
}

/**
 * Parses a key that came out of storage or off the wire. Throws on a malformed value so the
 * enclosing decode — which always runs inside a catching boundary — surfaces it as a typed failure
 * instead of carrying a wrong-but-valid key forward.
 */
fun DataByteArray.requireX25519PublicKey(): X25519PublicKey = X25519PublicKey.fromBytes(this).getOrThrow()

fun ByteArray.requireX25519PublicKey(): X25519PublicKey = toDataByteArray().requireX25519PublicKey()
