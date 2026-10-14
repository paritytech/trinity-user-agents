package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import java.security.SecureRandom

@JvmInline
value class HopTicket(val value: DataByteArray) {
    val bytes: ByteArray get() = value.value

    companion object {
        private const val TICKET_SIZE = 32

        fun random(): HopTicket {
            val bytes = ByteArray(TICKET_SIZE).also { SecureRandom().nextBytes(it) }
            return HopTicket(bytes.toDataByteArray())
        }

        fun fromRaw(bytes: ByteArray): HopTicket {
            return HopTicket(bytes.toDataByteArray())
        }
    }
}
