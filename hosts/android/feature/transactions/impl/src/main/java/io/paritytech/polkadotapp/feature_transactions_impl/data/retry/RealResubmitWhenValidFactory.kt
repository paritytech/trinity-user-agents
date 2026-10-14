package io.paritytech.polkadotapp.feature_transactions_impl.data.retry

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ExtrinsicSubmissionFailureRecoveryStrategy
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ResubmitWhenValidFactory
import javax.inject.Inject

class RealResubmitWhenValidFactory @Inject constructor(
    private val chainStateRepository: ChainStateRepository,
) : ResubmitWhenValidFactory {
    override fun create(chainId: ChainId, maxAttempts: Int?): ExtrinsicSubmissionFailureRecoveryStrategy {
        return ResubmitWhenValid(
            chainId = chainId,
            maxAttempts = maxAttempts,
            chainStateRepository = chainStateRepository,
        )
    }

    override fun createForTxInvalidation(chainId: ChainId, maxAttempts: Int?): ExtrinsicSubmissionFailureRecoveryStrategy {
        return CauseBasedRecoveryStrategy(create(chainId, maxAttempts))
    }
}
