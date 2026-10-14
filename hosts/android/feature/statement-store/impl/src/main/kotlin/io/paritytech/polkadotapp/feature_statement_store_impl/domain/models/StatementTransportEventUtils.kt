package io.paritytech.polkadotapp.feature_statement_store_impl.domain.models

import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry

fun List<StatementTransportEvent>.getRequest(): StatementTransportEvent.Request? {
    return this.filterIsInstance<StatementTransportEvent.Request>().firstOrNull()
}

fun List<StatementTransportEvent>.getRequests(): List<StatementTransportEvent.Request> {
    return this.filterIsInstance<StatementTransportEvent.Request>()
}

fun List<StatementTransportEvent>.getResponse(): StatementTransportEvent.Response? {
    return this.filterIsInstance<StatementTransportEvent.Response>().firstOrNull()
}

fun List<StatementTransportEvent>.hasRequest(): Boolean {
    return this.any { it is StatementTransportEvent.Request }
}

fun List<StatementTransportEvent>.hasResponse(): Boolean {
    return this.any { it is StatementTransportEvent.Response }
}

fun List<StatementTransportEvent>.defaultExpiry(): ULong {
    val lastExpiry = maxOfOrNull { it.expiry } ?: 0uL
    return StatementExpiry.nextAfter(lastExpiry)
}
