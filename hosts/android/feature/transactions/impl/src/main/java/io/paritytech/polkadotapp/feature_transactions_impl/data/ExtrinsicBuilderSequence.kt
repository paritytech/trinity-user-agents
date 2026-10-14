package io.paritytech.polkadotapp.feature_transactions_impl.data

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_transactions.api.data.SignerProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.submissionSigner
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.accountId

/**
 * Provides extrinsic builders with auto-incrementing nonces when the same origin
 * submits multiple extrinsics. Tracks (ChainId, AccountId) -> nonce sequence internally.
 * Create a new instance per submission batch.
 */
class ExtrinsicBuilderSequence(
    private val extrinsicBuilderFactory: ExtrinsicBuilderFactory,
    private val signerProvider: SignerProvider
) {
    private val iteratorCache = mutableMapOf<Pair<ChainId, AccountId?>, Iterator<ExtrinsicBuilderFactory.Submittable>>()

    suspend fun next(
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicBuilderFactory.Options
    ): ExtrinsicBuilderFactory.Submittable {
        val requestedSignerAccountId = origin.signerSource.accountId(chain)
        val signer = signerProvider.submissionSigner(origin.signerSource)

        val cacheKey = chain.id to requestedSignerAccountId
        val iterator = iteratorCache.getOrPut(cacheKey) {
            extrinsicBuilderFactory.createMultiForSubmission(
                chain = chain,
                signer = signer,
                requestedSignerAccountId = requestedSignerAccountId,
                options = options
            ).iterator()
        }
        return iterator.next()
    }
}
