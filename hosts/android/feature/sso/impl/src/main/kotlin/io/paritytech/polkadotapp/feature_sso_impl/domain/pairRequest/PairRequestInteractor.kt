package io.paritytech.polkadotapp.feature_sso_impl.domain.pairRequest

import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.RegisterDeviceProgress
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.RegisterDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.SyncDeviceProgress
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.SyncDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeOffer
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.DeviceOnboardingProgress
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.transformWhile
import javax.inject.Inject

class PairRequestInteractor @Inject constructor(
    private val registerDeviceUseCase: RegisterDeviceUseCase,
    private val syncDeviceUseCase: SyncDeviceUseCase,
) {
    context(diagnostics: StalenessReportCollector)
    fun approveHandshake(offer: HandshakeOffer): Flow<DeviceOnboardingProgress> {
        val deviceStatementAccountId = offer.device.statementAccountId

        return flow {
            emitAll(registerDeviceUseCase(offer).mapNotNull { it.toOnboarding() })
            emitAll(syncDeviceUseCase(deviceStatementAccountId).map { it.toOnboarding() })
        }.stopOnFailure()
    }

    private fun Flow<DeviceOnboardingProgress>.stopOnFailure(): Flow<DeviceOnboardingProgress> =
        transformWhile { progress ->
            emit(progress)
            progress !is DeviceOnboardingProgress.Failed
        }

    private fun RegisterDeviceProgress.toOnboarding(): DeviceOnboardingProgress? = when (this) {
        RegisterDeviceProgress.Verifying -> DeviceOnboardingProgress.Verifying
        RegisterDeviceProgress.Registering -> DeviceOnboardingProgress.Registering
        RegisterDeviceProgress.Done -> null
        is RegisterDeviceProgress.Failed -> DeviceOnboardingProgress.Failed(error)
    }

    private fun SyncDeviceProgress.toOnboarding(): DeviceOnboardingProgress = when (this) {
        SyncDeviceProgress.Syncing -> DeviceOnboardingProgress.Syncing
        SyncDeviceProgress.Done -> DeviceOnboardingProgress.Done
        is SyncDeviceProgress.Failed -> DeviceOnboardingProgress.Failed(error)
    }
}
