package io.paritytech.polkadotapp.feature_chats_impl.domain.devices

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactDevice
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactDevicesRepository
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.ContactDeviceProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RealContactDeviceProvider @Inject constructor(
    private val contactDevicesRepository: ContactDevicesRepository,
) : ContactDeviceProvider {
    override fun observeDevices(contactId: AccountId): Flow<List<DeviceInfo>> {
        return contactDevicesRepository.subscribeDevices(contactId).mapList { it.toDeviceInfo() }
    }

    override suspend fun getDevices(contactId: AccountId): List<DeviceInfo> {
        return contactDevicesRepository.getDevices(contactId).map { it.toDeviceInfo() }
    }

    private fun ContactDevice.toDeviceInfo() = DeviceInfo(
        statementAccountId = statementAccountId,
        encryptionPublicKey = encryptionPublicKey,
    )
}
