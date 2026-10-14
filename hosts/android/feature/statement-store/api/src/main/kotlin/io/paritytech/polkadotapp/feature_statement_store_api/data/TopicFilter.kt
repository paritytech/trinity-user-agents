package io.paritytech.polkadotapp.feature_statement_store_api.data

sealed class TopicFilter {
    data class MatchAll(val topics: List<StatementTopic>) : TopicFilter()
    data class MatchAny(val topics: List<StatementTopic>) : TopicFilter()
}
