package io.paritytech.polkadotapp.feature_wallet_impl.presentation

import io.paritytech.polkadotapp.feature_products_api.domain.FundingConfig
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.CompletableDeferred

// Mockito cannot suspend a stubbed suspend call.
internal class ParkedFundingDomainProvider : FundingDomainProvider {
    private val fundingConfig = FundingConfig(onrampUrl = ONRAMP_URL, offrampUrl = OFFRAMP_URL)
    private val reads = ArrayDeque<CompletableDeferred<Result<FundingConfig>>>()

    override suspend fun getFundingConfig(): Result<FundingConfig> {
        val read = CompletableDeferred<Result<FundingConfig>>()
        reads.addLast(read)
        return read.await()
    }

    override suspend fun getFundingProductIds(): Result<Set<ProductId>> = error("not reachable from the wallet screens")

    fun completeReadsWithConfig() = completeReads(Result.success(fundingConfig))

    fun failReads() = completeReads(Result.failure(IllegalStateException("no config")))

    private fun completeReads(result: Result<FundingConfig>) {
        while (reads.isNotEmpty()) {
            reads.removeFirst().complete(result)
        }
    }
}
