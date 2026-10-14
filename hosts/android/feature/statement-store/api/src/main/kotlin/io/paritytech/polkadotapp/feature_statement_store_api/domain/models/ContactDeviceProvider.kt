package io.paritytech.polkadotapp.feature_statement_store_api.domain.models

import io.paritytech.polkadotapp.common.domain.model.AccountId
import kotlinx.coroutines.flow.Flow

/**
 * Reactive source of a peer's devices (identity + registered extras).
 */
interface ContactDeviceProvider {
    fun observeDevices(contactId: AccountId): Flow<List<DeviceInfo>>

    suspend fun getDevices(contactId: AccountId): List<DeviceInfo>
}
