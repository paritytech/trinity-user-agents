package io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore

import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_api.getSyncedJsonObject
import javax.inject.Inject

interface AccountDataStoreConfigProvider {
    suspend fun contractAddress(): Result<EvmAccountId>
}

class RemoteConfigAccountDataStoreConfigProvider @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
) : AccountDataStoreConfigProvider {
    override suspend fun contractAddress(): Result<EvmAccountId> {
        return remoteConfigService.getSyncedJsonObject<AccountDataStoreConfigRemote>(CONFIG_KEY)
            .mapCatching { it.contractAddress.hexToDataByteArray() }
    }

    private companion object {
        const val CONFIG_KEY = "account_data_store_config"
    }
}

internal class AccountDataStoreConfigRemote(
    val contractAddress: HexString,
)
