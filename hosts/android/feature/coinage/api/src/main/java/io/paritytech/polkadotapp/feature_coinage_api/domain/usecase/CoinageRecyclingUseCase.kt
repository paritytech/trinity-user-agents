package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import kotlinx.coroutines.flow.Flow

interface CoinageRecyclingUseCase {
    /** Recycles [coins] under a group nobody follows. */
    suspend fun recycle(coins: List<Coin>): Result<Unit>

    /**
     * Recycles [coins] under [groupId]. A group that already holds transactions was submitted by an earlier
     * attempt and is left alone, so retrying with the same [groupId] never recycles twice.
     */
    suspend fun recycle(coins: List<Coin>, groupId: CoinageOperationGroupId): Result<Unit>

    /**
     * What became of the recycling registered under [groupId]. Only meaningful once [recycle] returned for it:
     * a group with nothing registered reads as [RecyclingStatus.Incomplete].
     */
    fun observeRecyclingStatus(groupId: CoinageOperationGroupId): Flow<RecyclingStatus>
}
