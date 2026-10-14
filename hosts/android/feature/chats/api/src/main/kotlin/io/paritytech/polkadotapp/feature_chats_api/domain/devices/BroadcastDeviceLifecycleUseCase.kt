package io.paritytech.polkadotapp.feature_chats_api.domain.devices

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector

interface BroadcastDeviceLifecycleUseCase {
    context(diagnostics: StalenessReportCollector)
    suspend fun broadcastDeviceAdded(
        statementAccountId: AccountId,
        encryptionPublicKey: X25519PublicKey,
    ): Result<Unit>

    suspend fun broadcastDeviceRemoved(statementAccountId: AccountId): Result<Unit>

    /**
     * Targeted send of `DeviceAdded` to a single contact. Used by the post-acceptance
     * fan-out flow to backfill a newly added contact with our full device list.
     */
    suspend fun sendDeviceAddedTo(
        contactAccountId: AccountId,
        statementAccountId: AccountId,
        encryptionPublicKey: X25519PublicKey,
    ): Result<Unit>
}
