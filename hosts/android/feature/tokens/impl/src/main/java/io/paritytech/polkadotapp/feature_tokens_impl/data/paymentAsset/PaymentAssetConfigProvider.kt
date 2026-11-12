package io.paritytech.polkadotapp.feature_tokens_impl.data.paymentAsset

import com.google.gson.Gson
import io.paritytech.polkadotapp.feature_tokens_impl.data.mappers.toDomain
import io.paritytech.polkadotapp.feature_tokens_impl.domain.paymentAsset.PaymentAssetConfig
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

internal interface PaymentAssetConfigProvider {
    suspend fun lastActivatedPaymentAssetConfig(): Result<PaymentAssetConfig?>

    suspend fun paymentAssetConfig(): Result<PaymentAssetConfig?>
}

internal class RemoteConfigPaymentAssetConfigProvider @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
    private val gson: Gson,
) : PaymentAssetConfigProvider {
    override suspend fun lastActivatedPaymentAssetConfig(): Result<PaymentAssetConfig?> {
        return remoteConfigService.getString(CONFIG_KEY).mapCatching { it.toConfig() }
    }

    override suspend fun paymentAssetConfig(): Result<PaymentAssetConfig?> {
        return remoteConfigService.getSyncedString(CONFIG_KEY).mapCatching { it.toConfig() }
    }

    // An unset key reads back as an empty string, which Gson parses to null.
    private fun String.toConfig(): PaymentAssetConfig? =
        gson.fromJson(this, PaymentAssetConfigRemote::class.java)?.toDomain()

    private companion object {
        const val CONFIG_KEY = "payment_asset_config"
    }
}
