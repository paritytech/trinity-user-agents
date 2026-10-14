package io.paritytech.polkadotapp.feature_sso_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId

/** Per-device sync checkpoint bookkeeping. Read-side data is on [GetActiveSsoSessionsUseCase]. */
interface OwnDevicesJournal {
    /** `null` = no checkpoint yet (treat as 0). */
    suspend fun getOutgoingUpdateTime(deviceStatementAccountId: AccountId): Long?

    suspend fun updateOutgoingUpdateTime(deviceStatementAccountId: AccountId, timePoint: Long)

    /** Last sync-signaling offerId used with this device; `null` until the first connection attempt. */
    suspend fun getLastSyncOfferId(deviceStatementAccountId: AccountId): String?

    suspend fun saveLastSyncOfferId(deviceStatementAccountId: AccountId, offerId: String)
}
