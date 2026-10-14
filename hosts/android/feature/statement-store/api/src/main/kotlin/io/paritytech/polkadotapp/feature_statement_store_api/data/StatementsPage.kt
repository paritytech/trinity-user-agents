package io.paritytech.polkadotapp.feature_statement_store_api.data

data class StatementsPage(
    val statements: List<Statement>,
    val isComplete: Boolean,
)
