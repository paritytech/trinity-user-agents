package io.paritytech.polkadotapp.feature_statement_store_impl.data.models.requests

import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest

class SubmitStatementRequest(statement: String) : RuntimeRequest(
    method = "statement_submit",
    params = listOf(statement)
)
