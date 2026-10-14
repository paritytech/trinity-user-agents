package io.paritytech.polkadotapp.feature_fund_impl.domain.fund

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.asset
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.chainWithAsset
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getDepositAccount
import io.paritytech.polkadotapp.feature_fund_api.domain.AutoConvertDepositService
import io.paritytech.polkadotapp.feature_fund_api.domain.model.AutoConvertDeposit
import io.paritytech.polkadotapp.feature_fund_api.domain.model.DepositTerms
import io.paritytech.polkadotapp.feature_fund_impl.domain.fund.model.FundCredentials
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

interface FundInteractor {
    fun currentDeposit(): Flow<AutoConvertDeposit?>

    suspend fun fundingCredentials(assetId: FullChainAssetId): FundCredentials

    context(scope: ComputationalScope)
    suspend fun depositTerms(fullChainAssetId: FullChainAssetId): Result<DepositTerms>
}

class RealFundInteractor @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val accountRepository: AccountRepository,
    private val autoConvertDepositService: AutoConvertDepositService,
) : FundInteractor {
    override fun currentDeposit() = autoConvertDepositService.currentDeposit

    context(scope: ComputationalScope)
    override suspend fun depositTerms(fullChainAssetId: FullChainAssetId): Result<DepositTerms> {
        val asset = chainRegistry.asset(fullChainAssetId)
        return autoConvertDepositService.depositTerms(asset)
    }

    override suspend fun fundingCredentials(assetId: FullChainAssetId): FundCredentials {
        val depositAccount = accountRepository.getDepositAccount()
        return FundCredentials(
            depositAccount = depositAccount,
            chainWithAsset = chainRegistry.chainWithAsset(assetId)
        )
    }
}
