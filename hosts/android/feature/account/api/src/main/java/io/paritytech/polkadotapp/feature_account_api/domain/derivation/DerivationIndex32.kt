package io.paritytech.polkadotapp.feature_account_api.domain.derivation

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.common.utils.endsWith
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * RFC-0022 account selector within a derivation subtree. Always 32 bytes, so it can be used
 * directly as a junction chain code.
 */
@JvmInline
value class DerivationIndex32 private constructor(val bytes: DataByteArray) {
    /**
     * Renders the index as a path segment. Hex is what keeps the 32 bytes intact:
     * [io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder] maps a hex
     * segment straight to the raw chain code, skipping the string normalization other forms go through.
     */
    fun asPathSegment(): String = bytes.value.toHexString(withPrefix = true)

    /**
     * The plain index this was built from, or null when the selector is a raw 32-byte value.
     * Plain indices keep a product's accounts enumerable and are the form products are expected to use.
     */
    fun asUIntOrNull(): UInt? {
        val raw = bytes.value
        if (!raw.endsWith(INDEX_MAGIC)) return null

        return ByteBuffer.wrap(raw, 0, UInt.SIZE_BYTES)
            .order(ByteOrder.LITTLE_ENDIAN)
            .int
            .toUInt()
    }

    companion object {
        const val SIZE_BYTES = 32

        private const val MAGIC_SIZE_BYTES = 28

        /**
         * Keeps plain indices and raw selectors in separate spaces: a raw value only collides with
         * an index if it happens to end in the magic.
         */
        val INDEX_MAGIC: ByteArray = "product-account-index".encodeToByteArray()
            .blake2b256()
            .copyOf(MAGIC_SIZE_BYTES)

        fun fromUInt(index: UInt): DerivationIndex32 {
            val indexBytes = ByteBuffer.allocate(UInt.SIZE_BYTES)
                .order(ByteOrder.LITTLE_ENDIAN)
                .putInt(index.toInt())
                .array()

            return DerivationIndex32((indexBytes + INDEX_MAGIC).toDataByteArray())
        }

        fun fromBytes(bytes: DataByteArray): Result<DerivationIndex32> {
            if (bytes.value.size != SIZE_BYTES) {
                return Result.failure(IllegalArgumentException("DerivationIndex32 must be $SIZE_BYTES bytes, got ${bytes.value.size}"))
            }

            return Result.success(DerivationIndex32(bytes))
        }

        fun default(): DerivationIndex32 = fromUInt(0u)
    }
}

/**
 * Renders the selector the way it was most likely authored: a plain index as a decimal number,
 * a raw selector as hex.
 */
fun DerivationIndex32.asDisplayString(): String = asUIntOrNull()?.toString() ?: asPathSegment()
