package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import kotlinx.coroutines.flow.Flow

/** Where a claim or top-up the app started in the wallet stands. */
interface CoinageOperationStatusUseCase {
    /**
     * Follows the operation started under [groupId] from where it stands now until it ends. Fails when nothing
     * was started under it.
     */
    fun follow(groupId: CoinageOperationGroupId): Flow<CoinageTransferDetection>
}
