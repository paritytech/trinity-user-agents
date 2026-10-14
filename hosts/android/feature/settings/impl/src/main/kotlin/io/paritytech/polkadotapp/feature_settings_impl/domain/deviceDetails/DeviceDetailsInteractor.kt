package io.paritytech.polkadotapp.feature_settings_impl.domain.deviceDetails

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.UnregisterDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class DeviceDetailsInteractor @Inject constructor(
    private val getActiveSsoSessionsUseCase: GetActiveSsoSessionsUseCase,
    private val unregisterDeviceUseCase: UnregisterDeviceUseCase,
) {
    fun observeDevice(deviceId: String): Flow<ActiveSsoSession?> {
        return getActiveSsoSessionsUseCase.observeSessions().map { sessions ->
            sessions.firstOrNull { it.id == deviceId }
        }
    }

    suspend fun removeDevice(statementAccountId: AccountId): Result<Unit> {
        return unregisterDeviceUseCase(statementAccountId)
    }
}
