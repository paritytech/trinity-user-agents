package io.paritytech.polkadotapp.feature_fund_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_fund_api.domain.model.AutoConvertDeposit
import io.paritytech.polkadotapp.feature_fund_api.domain.model.DepositTerms
import kotlinx.coroutines.flow.Flow

interface AutoConvertDepositService {
    /**
     * Flow of current deposit
     *
     * Should return following values:
     * - null when there is no deposit
     * - deposit with Status.InProgress when it is in progress
     * - deposit with Status.Done when it is done
     * - deposit with Status.Failure when it is failed
     *
     * Once deposit with Status.Done was emitted,
     * it should emit null to signal that deposit is finished
     */
    val currentDeposit: Flow<AutoConvertDeposit?>

    /**
     * Should be called once per session
     */
    context(scope: ComputationalScope)
    suspend fun startObserveAndConvert()

    context(scope: ComputationalScope)
    fun initiateDepositTermsWarmUp()

    context(scope: ComputationalScope)
    suspend fun depositTerms(chainAsset: Chain.Asset): Result<DepositTerms>
}
