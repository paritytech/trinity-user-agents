package io.paritytech.polkadotapp.feature_settings_impl.domain.linkedDevices

import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class LinkedDevicesInteractor @Inject constructor(
    private val getActiveSsoSessionsUseCase: GetActiveSsoSessionsUseCase
) {
    fun observeLinkedDevices(): Flow<List<ActiveSsoSession>> {
        return getActiveSsoSessionsUseCase.observeSessions()
    }
}
