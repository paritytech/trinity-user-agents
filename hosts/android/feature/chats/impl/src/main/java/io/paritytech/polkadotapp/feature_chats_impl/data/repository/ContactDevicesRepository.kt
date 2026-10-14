package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.database.dao.ContactDeviceDao
import io.paritytech.polkadotapp.database.model.ContactDeviceLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactDevice
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

interface ContactDevicesRepository {
    suspend fun getDevices(contactAccountId: ContactAccountId): List<ContactDevice>

    fun subscribeDevices(contactAccountId: ContactAccountId): Flow<List<ContactDevice>>

    fun subscribeAllDevices(): Flow<Map<ContactAccountId, List<ContactDevice>>>

    suspend fun addDevice(device: ContactDevice)

    suspend fun removeDevice(contactAccountId: ContactAccountId, statementAccountId: AccountId)

    suspend fun clearDevicesFor(contactAccountId: ContactAccountId)
}

class RealContactDevicesRepository @Inject constructor(
    private val dao: ContactDeviceDao
) : ContactDevicesRepository {
    override suspend fun getDevices(contactAccountId: ContactAccountId): List<ContactDevice> {
        return dao.getByContact(contactAccountId.value).map { it.toDomain() }
    }

    override fun subscribeDevices(contactAccountId: ContactAccountId): Flow<List<ContactDevice>> {
        return dao.subscribeByContact(contactAccountId.value)
            .mapList { it.toDomain() }
    }

    override fun subscribeAllDevices(): Flow<Map<ContactAccountId, List<ContactDevice>>> {
        return dao.subscribeAll()
            .mapList { it.toDomain() }
            .map { devices -> devices.groupBy { it.contactAccountId } }
    }

    override suspend fun addDevice(device: ContactDevice) {
        dao.upsert(device.toLocal())
    }

    override suspend fun removeDevice(
        contactAccountId: ContactAccountId,
        statementAccountId: AccountId
    ) {
        dao.delete(contactAccountId.value, statementAccountId.value)
    }

    override suspend fun clearDevicesFor(contactAccountId: ContactAccountId) {
        dao.deleteAllForContact(contactAccountId.value)
    }
}

private fun ContactDeviceLocal.toDomain(): ContactDevice {
    return ContactDevice(
        contactAccountId = contactAccountId.intoAccountId(),
        statementAccountId = statementAccountId.intoAccountId(),
        encryptionPublicKey = encryptionPublicKey.requireX25519PublicKey(),
    )
}

private fun ContactDevice.toLocal(): ContactDeviceLocal {
    return ContactDeviceLocal(
        contactAccountId = contactAccountId.value,
        statementAccountId = statementAccountId.value,
        encryptionPublicKey = encryptionPublicKey.bytes.value,
    )
}
