package io.paritytech.polkadotapp.feature_transfers_api.domain.usecase

import io.paritytech.polkadotapp.chains.multiNetwork.ChainWithAsset
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId

interface TestnetFundUseCase {
    suspend operator fun invoke(chainWithAsset: ChainWithAsset, amount: Balance, to: AccountId): Result<Unit>
}
