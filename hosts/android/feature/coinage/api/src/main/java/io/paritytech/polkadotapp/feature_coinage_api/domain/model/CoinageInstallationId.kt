package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import java.security.SecureRandom

/**
 * The RFC-0017 `page` a single app installation allocates its coins and vouchers under.
 *
 * Every installation draws its own, so a reinstall can never re-issue a key an earlier one already handed off.
 */
@JvmInline
value class CoinageInstallationId(val value: DataByteArray) {
    init {
        require(value.value.size == SIZE_BYTES) { "Installation id must be $SIZE_BYTES bytes, got ${value.value.size}" }
    }

    companion object {
        const val SIZE_BYTES = 32

        fun random(): CoinageInstallationId {
            val bytes = ByteArray(SIZE_BYTES).also { SecureRandom().nextBytes(it) }
            return CoinageInstallationId(bytes.toDataByteArray())
        }
    }
}
