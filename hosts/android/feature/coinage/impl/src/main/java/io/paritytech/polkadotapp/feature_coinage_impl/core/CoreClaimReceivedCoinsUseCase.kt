package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ClaimReceivedCoinsUseCase
import kotlinx.coroutines.flow.Flow
import uniffi.truapi.Retry
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class CoreClaimReceivedCoinsUseCase @Inject constructor(
    private val core: CoinageCore,
) : ClaimReceivedCoinsUseCase {
    override fun claim(
        coinKeys: List<CoinPrivateKey>,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection> {
        val id = CoinageIds.of(groupId)
        return core.followOperation(id) { coinage ->
            coinage.import(id, Retry.Until(retryUntil.epochSeconds.toULong()), coinKeys.map { it.value })
        }
    }
}
