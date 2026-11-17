package io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model

import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId

/** The id of an operation the app runs in the wallet, which a retry under the same id joins. */
typealias CoinageOperationGroupId = OperationGroupId
