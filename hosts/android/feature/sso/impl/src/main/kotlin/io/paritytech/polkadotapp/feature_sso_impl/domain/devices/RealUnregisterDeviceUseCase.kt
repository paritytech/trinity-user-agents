package io.paritytech.polkadotapp.feature_sso_impl.domain.devices

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_chats_api.domain.devices.BroadcastDeviceLifecycleUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.UnregisterDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_impl.data.repository.SsoSessionRepository
import io.paritytech.polkadotapp.feature_sso_impl.domain.SsoService
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocator
import javax.inject.Inject

class RealUnregisterDeviceUseCase @Inject constructor(
    private val slotAllocator: StatementStoreSlotAllocator,
    private val ssoService: SsoService,
    private val ssoSessionRepository: SsoSessionRepository,
    private val broadcastDeviceLifecycleUseCase: BroadcastDeviceLifecycleUseCase,
) : UnregisterDeviceUseCase {
    override suspend fun invoke(statementAccountId: AccountId): Result<Unit> {
        return deallocateSlot(statementAccountId)
            .flatMap { disconnectSession(statementAccountId) }
            .flatMap { broadcastDeviceRemoved(statementAccountId) }
    }

    private suspend fun deallocateSlot(statementAccountId: AccountId): Result<Unit> {
        return slotAllocator.deallocateAllSlots(statementAccountId)
    }

    private suspend fun disconnectSession(statementAccountId: AccountId): Result<Unit> = runCatching {
        val sessionId = ssoSessionRepository.getSessionByStatementAccountId(statementAccountId)?.id
            ?: return@runCatching
        ssoService.disconnectSession(sessionId)
    }

    private suspend fun broadcastDeviceRemoved(statementAccountId: AccountId): Result<Unit> {
        return broadcastDeviceLifecycleUseCase.broadcastDeviceRemoved(statementAccountId)
    }
}
