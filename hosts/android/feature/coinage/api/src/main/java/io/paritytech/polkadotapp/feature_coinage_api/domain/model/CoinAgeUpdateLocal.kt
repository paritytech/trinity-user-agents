package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId

/**
 * What one look at the chain says about a coin.
 *
 * [onChain] is the whole of the presence answer; [age] is only carried when the chain gave one, and never
 * un-tells what an earlier look established.
 */
class CoinUpdate(
    val accountId: AccountId,
    val onChain: Boolean,
    val age: Int?,
)
