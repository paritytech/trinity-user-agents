package io.paritytech.polkadotapp.feature_swap_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import javax.inject.Inject

interface AssetInAdditionalSwapDeductionUseCase {
    suspend fun invoke(
        assetIn: Chain.Asset,
        assetOut: Chain.Asset,
        metaAccount: MetaAccount,
    ): Balance
}

class RealAssetInAdditionalSwapDeductionUseCase @Inject constructor(
    private val balanceTypeRegistry: TokenBalanceTypeRegistry,
    private val chainRegistry: ChainRegistry
) : AssetInAdditionalSwapDeductionUseCase {
    override suspend fun invoke(
        assetIn: Chain.Asset,
        assetOut: Chain.Asset,
        metaAccount: MetaAccount,
    ): Balance {
        val originChain = chainRegistry.getChain(assetIn.chainId)
        val accountId = metaAccount.accountIdIn(originChain)

        val assetInBalanceType = balanceTypeRegistry.typeFor(assetIn)

        val assetInBalanceCanDropBelowEd =
            assetInBalanceType.totalCanDropBelowMinimumBalance(accountId)

        val sameChain = assetIn.chainId == assetOut.chainId

        val assetOutCanProvideSufficiency = sameChain && assetInBalanceType.isSelfSufficient()

        val canDustAssetIn = assetInBalanceCanDropBelowEd || assetOutCanProvideSufficiency
        val shouldKeepEdForAssetIn = !canDustAssetIn

        return if (shouldKeepEdForAssetIn) {
            assetInBalanceType.minimumBalance()
        } else {
            Balance.ZERO
        }
    }
}
