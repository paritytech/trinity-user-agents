package io.paritytech.polkadotapp.feature_sso_impl.domain.devices

import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_chats_api.domain.devices.BroadcastDeviceLifecycleUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.HandshakeResponse
import io.paritytech.polkadotapp.feature_sso_api.domain.SsoHandshakeUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.RegisterDeviceProgress
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.RegisterDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeOffer
import io.paritytech.polkadotapp.feature_sso_impl.data.repository.SsoSessionRepository
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.SsoSessionData
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocator
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import timber.log.Timber
import javax.inject.Inject

class RealRegisterDeviceUseCase @Inject constructor(
    private val slotAllocator: StatementStoreSlotAllocator,
    private val ssoSessionRepository: SsoSessionRepository,
    private val ssoHandshakeUseCase: SsoHandshakeUseCase,
    private val broadcastDeviceLifecycleUseCase: BroadcastDeviceLifecycleUseCase,
) : RegisterDeviceUseCase {
    context(diagnostics: StalenessReportCollector)
    override fun invoke(offer: HandshakeOffer): Flow<RegisterDeviceProgress> = flow {
        val deviceStatementAccountId = offer.device.statementAccountId
        val deviceEncryptionPublicKey = offer.device.encryptionPublicKey
        Timber.d("register device $deviceStatementAccountId (session='${offer.metadata.hostName}')")

        emit(RegisterDeviceProgress.Verifying)

        // No region for the Verifying/Registering phases themselves - `ProgressCard` on the pair screen already names
        // them, so the report only covers the steps that progress model cannot see
        ssoHandshakeUseCase.respondToOffer(offer, HandshakeResponse.AllowanceAllocation)
            .flatMap { slotAllocator.allocate(deviceStatementAccountId, strategy = OnExistingAllocationStrategy.INCREASE, priority = SlotPriority.High) }
            .flatMap { persistDeviceSession(offer) }
            .flatMap {
                emit(RegisterDeviceProgress.Registering)

                ssoHandshakeUseCase.respondToOffer(offer, HandshakeResponse.Success)
                    .flatMap {
                        broadcastDeviceLifecycleUseCase.broadcastDeviceAdded(
                            statementAccountId = deviceStatementAccountId,
                            encryptionPublicKey = deviceEncryptionPublicKey,
                        )
                    }
            }
            .logFailure("device registration failed for device $deviceStatementAccountId")
            .onFailure { throwable ->
                ssoHandshakeUseCase.respondToOffer(offer, HandshakeResponse.Failure(throwable.toFailureReason()))
                    .logFailure("failed to deliver Failure(reason) for device $deviceStatementAccountId")
                emit(RegisterDeviceProgress.Failed(throwable))
                return@flow
            }

        emit(RegisterDeviceProgress.Done)
    }

    private suspend fun persistDeviceSession(
        offer: HandshakeOffer
    ): Result<Unit> = runCatching {
        val statementStorePublicKey = EncodedPublicKey(offer.device.statementAccountId.value)
        ssoSessionRepository.saveSession(
            SsoSessionData(
                sharedSecretPublicKey = offer.device.encryptionPublicKey,
                statementStorePublicKey = statementStorePublicKey,
                metadata = offer.metadata,
                addedAt = System.currentTimeMillis(),
                status = DeviceStatus.ACTIVE,
                lastUpdate = System.currentTimeMillis(),
                outgoingUpdateTime = null,
                lastSyncOfferId = null,
            )
        )
    }

    private fun Throwable.toFailureReason(): String {
        return message ?: this::class.simpleName ?: "unknown failure"
    }
}
