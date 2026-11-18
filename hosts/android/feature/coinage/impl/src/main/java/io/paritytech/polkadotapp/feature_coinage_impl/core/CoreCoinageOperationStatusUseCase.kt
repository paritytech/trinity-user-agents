package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageOperationStatusUseCase
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class CoreCoinageOperationStatusUseCase @Inject constructor(
    private val core: CoinageCore,
) : CoinageOperationStatusUseCase {
    override fun follow(groupId: CoinageOperationGroupId): Flow<CoinageTransferDetection> = core.followStarted(CoinageIds.of(groupId))
}
