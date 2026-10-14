package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent

/**
 * Builds an outgoing [Statement.Body] from a [StatementTransportEvent]. Implementations pick
 * the wire format (plain pairwise vs. multi-device envelope).
 */
interface OutgoingBodyBuilder {
    suspend fun buildRequestBody(request: StatementTransportEvent.Request): Statement.Body

    suspend fun buildResponseBody(response: StatementTransportEvent.Response): Statement.Body
}
