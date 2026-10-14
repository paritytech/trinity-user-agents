package io.paritytech.polkadotapp.feature_sso_api.domain.devices

import io.paritytech.polkadotapp.common.domain.model.AccountId
import kotlinx.coroutines.flow.Flow

interface SyncDeviceUseCase {
    operator fun invoke(deviceStatementAccountId: AccountId): Flow<SyncDeviceProgress>
}
