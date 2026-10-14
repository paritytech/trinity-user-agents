package io.paritytech.polkadotapp.feature_statement_store_impl.data.models.requests

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter

class SubscribeStatementRequest(
    topicFilter: TopicFilter,
) : RuntimeRequest(
    method = "statement_subscribeStatement",
    params = listOf(topicFilter.toJsonParam())
)

private fun TopicFilter.toJsonParam(): Any = when (this) {
    is TopicFilter.MatchAll -> mapOf("matchAll" to topics.map { it.toHexString(withPrefix = true) })
    is TopicFilter.MatchAny -> mapOf("matchAny" to topics.map { it.toHexString(withPrefix = true) })
}
