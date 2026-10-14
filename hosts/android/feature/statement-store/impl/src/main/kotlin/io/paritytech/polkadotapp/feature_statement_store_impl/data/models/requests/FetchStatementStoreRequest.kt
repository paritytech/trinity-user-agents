@file:OptIn(ExperimentalUnsignedTypes::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.data.models.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

class FetchStatementStoreRequest(
    filters: List<ByteArray>,
) : RuntimeRequest(
    method = "statement_broadcastsStatement",
    params = listOf(
        filters
            .map { it.toUByteArray() }
            .map { uBytes -> uBytes.map { it.toInt() } }
    )
)
