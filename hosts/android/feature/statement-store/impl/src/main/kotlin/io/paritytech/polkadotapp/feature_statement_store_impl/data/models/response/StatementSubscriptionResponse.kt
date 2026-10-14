package io.paritytech.polkadotapp.feature_statement_store_impl.data.models.response

class StatementSubscriptionResponse(
    val event: String,
    val data: Data
) {
    class Data(
        val statements: List<String>,
        val remaining: Int
    )
}
