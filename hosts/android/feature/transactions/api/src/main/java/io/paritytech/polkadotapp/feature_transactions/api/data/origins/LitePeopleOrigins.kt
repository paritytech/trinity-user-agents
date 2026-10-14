package io.paritytech.polkadotapp.feature_transactions.api.data.origins

import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

interface LitePeopleOrigins {
    suspend fun asLitePerson(): TransactionOrigin
}
