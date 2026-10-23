package io.paritytech.polkadotapp.feature_statement_store_impl.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.ContactDeviceProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import javax.inject.Inject

/**
 * The app keeps no devices of a peer: the native person chat, which learned them, is gone, and the chat product keeps
 * its contacts' devices itself. A session the app opens (SSO, for example) reaches the peer's own account only.
 */
class NoContactDeviceProvider @Inject constructor() : ContactDeviceProvider {
    override fun observeDevices(contactId: AccountId): Flow<List<DeviceInfo>> = flowOf(emptyList())

    override suspend fun getDevices(contactId: AccountId): List<DeviceInfo> = emptyList()
}
