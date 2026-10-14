package io.paritytech.polkadotapp.feature_coinage_api.domain.service

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import kotlinx.coroutines.flow.Flow

interface CoinageAccountBackupObserver {
    /** The on-chain registration of this installation, for surfacing a backup that has not yet landed. */
    fun subscribeStatus(): Flow<CoinageAccountBackupStatus>
}
