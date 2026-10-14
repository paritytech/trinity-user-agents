package io.paritytech.polkadotapp.feature_chats_api.domain.devices

import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo

interface OurDevicesProvider {
    suspend fun getOurDevices(): List<DeviceInfo>
}
