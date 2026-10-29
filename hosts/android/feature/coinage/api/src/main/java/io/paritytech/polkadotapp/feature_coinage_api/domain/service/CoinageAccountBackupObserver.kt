package io.paritytech.polkadotapp.feature_coinage_api.domain.service

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import kotlinx.coroutines.flow.Flow

interface CoinageAccountBackupObserver {
    /** The on-chain registration of this installation, for surfacing a backup that has not yet landed. */
    fun subscribeStatus(): Flow<CoinageAccountBackupStatus>

    /** Whether a coin or voucher allocation is currently held back until the registration completes. */
    fun subscribeAllocationWaiting(): Flow<Boolean>
}
