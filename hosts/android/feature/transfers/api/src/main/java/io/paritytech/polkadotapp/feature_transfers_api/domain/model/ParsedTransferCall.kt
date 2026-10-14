package io.paritytech.polkadotapp.feature_transfers_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId

class ParsedTransferCall(
    val recipient: AccountId,
    val amount: Balance,
)
