package io.paritytech.polkadotapp.feature_statement_store_impl.domain.models

import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementResponseCode

sealed interface StatementTransportEvent {
    val requestId: String
    val expiry: ULong

    class Request(
        override val requestId: String,
        override val expiry: ULong,
        val messages: List<EncodedMessage>
    ) : StatementTransportEvent

    class Response(
        override val requestId: String,
        override val expiry: ULong,
        val responseCode: StatementResponseCode
    ) : StatementTransportEvent
}
