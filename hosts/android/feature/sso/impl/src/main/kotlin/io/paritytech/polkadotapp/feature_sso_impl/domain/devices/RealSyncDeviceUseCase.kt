package io.paritytech.polkadotapp.feature_sso_impl.domain.devices

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.SyncDeviceProgress
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.SyncDeviceUseCase
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import javax.inject.Inject

class RealSyncDeviceUseCase @Inject constructor() : SyncDeviceUseCase {
    // TODO replace stub with real device-sync logic when it is available
    override fun invoke(deviceStatementAccountId: AccountId): Flow<SyncDeviceProgress> = flow {
        emit(SyncDeviceProgress.Syncing)
        delay(SYNC_STUB_DELAY_MS)
        emit(SyncDeviceProgress.Done)
    }

    private companion object {
        const val SYNC_STUB_DELAY_MS = 800L
    }
}
