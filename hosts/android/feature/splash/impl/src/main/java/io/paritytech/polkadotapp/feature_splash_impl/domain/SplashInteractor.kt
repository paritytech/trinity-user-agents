package io.paritytech.polkadotapp.feature_splash_impl.domain

import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class SplashInteractor @Inject constructor(
    private val observeAccountOnboardingStatusUseCase: ObserveAccountOnboardingStatusUseCase,
    private val remoteConfigService: RemoteConfigService,
    private val networkStateService: NetworkStateService,
) {
    fun observeAccountOnboardingStatus(): Flow<AccountOnboardingStatus> =
        observeAccountOnboardingStatusUseCase()

    // TODO: remove this temporary solution later
    suspend fun syncRemoteConfigWhenOnline() {
        networkStateService.isNetworkAvailable
            .filter { it }
            .map { remoteConfigService.sync() }
            .first { it.isSuccess }
    }
}
