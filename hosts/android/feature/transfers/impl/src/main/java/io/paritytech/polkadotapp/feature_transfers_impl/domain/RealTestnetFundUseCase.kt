package io.paritytech.polkadotapp.feature_transfers_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainWithAsset
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.TestnetTransactionOrigins
import io.paritytech.polkadotapp.feature_transfers_api.data.type.TokenTransfersTypeRegistry
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.TransferArguments
import io.paritytech.polkadotapp.feature_transfers_api.domain.usecase.TestnetFundUseCase
import javax.inject.Inject

class RealTestnetFundUseCase @Inject constructor(
    private val transfersTypeRegistry: TokenTransfersTypeRegistry,
    private val testnetTransactionOrigins: TestnetTransactionOrigins,
) : TestnetFundUseCase {
    override suspend fun invoke(chainWithAsset: ChainWithAsset, amount: Balance, to: AccountId): Result<Unit> {
        return transfersTypeRegistry.typeFor(chainWithAsset.asset)
            .performAndTrackTransfer(
                TransferArguments(
                    recipient = to,
                    origin = testnetTransactionOrigins.fundingOrigin(),
                    amount = amount
                )
            )
            .flattenExecutionFailure()
            .coerceToUnit()
    }
}
