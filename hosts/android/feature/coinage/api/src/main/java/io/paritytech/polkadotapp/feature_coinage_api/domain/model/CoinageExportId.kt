package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.blake2b256

/** The 32-byte id a payment's coins are handed out under, which its status is followed by. */
@JvmInline
value class CoinageExportId(val value: DataByteArray) {
    companion object {
        /** A stable id for [label], so that a retry joins the payment instead of starting a second one. */
        fun of(label: String) = CoinageExportId(label.encodeToByteArray().blake2b256().toDataByteArray())

        /** The id of the payment the chat message [messageId] carries, so it can be followed from the message alone. */
        fun ofChatPayment(messageId: String) = of("chat-send:$messageId")
    }
}
