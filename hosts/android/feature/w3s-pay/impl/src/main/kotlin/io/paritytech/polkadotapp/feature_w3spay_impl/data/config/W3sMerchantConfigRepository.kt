package io.paritytech.polkadotapp.feature_w3spay_impl.data.config

import androidx.annotation.Keep
import com.google.gson.Gson
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.utils.decodeFormBase64UrlSafe
import io.paritytech.polkadotapp.common.utils.fromJson
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

/**
 * A merchant ("cash register") registered in the `w3s-merchants` Firebase remote config entry.
 *
 * @param topic the 32-byte statement store topic the merchant listens on
 * @param key the X25519 public key the payment payload is encrypted to
 * @param name optional human-readable merchant name shown to the payer; falls back to the serial
 */
class W3sMerchant(
    val topic: ByteArray,
    val key: X25519PublicKey,
    val name: String?,
)

interface W3sMerchantConfigRepository {
    /**
     * Resolves the merchant registered under [serialNumber] (the DSFinV-K KASSEN_SERIENNUMMER).
     * Failure when the config cannot be fetched/parsed; success with `null` when there is simply no
     * entry for [serialNumber].
     */
    suspend fun merchantFor(serialNumber: String): Result<W3sMerchant?>
}

private const val REMOTE_CONFIG_KEY = "w3s_merchants"
private const val TOPIC_SIZE = 32

class RealW3sMerchantConfigRepository @Inject constructor(
    private val gson: Gson,
    private val remoteConfig: RemoteConfigService,
) : W3sMerchantConfigRepository {
    override suspend fun merchantFor(serialNumber: String): Result<W3sMerchant?> {
        return remoteConfig.getSyncedString(REMOTE_CONFIG_KEY)
            .mapCatching { raw -> gson.fromJson<Map<String, W3sMerchantRemote>>(raw)[serialNumber]?.toDomain() }
    }

    private fun W3sMerchantRemote.toDomain(): W3sMerchant {
        val topicBytes = topic.decodeFormBase64UrlSafe()
        require(topicBytes.size == TOPIC_SIZE) { "W3S merchant topic must be exactly $TOPIC_SIZE bytes" }

        // Same check W3sCoinsSubmitter applies at submit time, so validation cannot drift from
        // the key type the payload is actually encrypted to.
        val merchantKey = key.decodeFormBase64UrlSafe().requireX25519PublicKey()

        return W3sMerchant(topic = topicBytes, key = merchantKey, name = name)
    }

    @Keep
    private class W3sMerchantRemote(val topic: String, val key: String, val name: String?)
}
