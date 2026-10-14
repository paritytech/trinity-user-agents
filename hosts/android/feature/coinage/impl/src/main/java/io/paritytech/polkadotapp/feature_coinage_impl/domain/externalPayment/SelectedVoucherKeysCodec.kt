package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray
import javax.inject.Inject

class SelectedVoucherKeysCodec @Inject constructor() {
    fun encode(keys: List<CoinageKeyIndex>): String {
        val scale = keys.map { SelectedVoucherKeyScale(it.installation.value, it.item) }
        return BinaryScale.encodeToByteArray(scale).toHexString(withPrefix = true)
    }

    fun decode(stored: String): List<CoinageKeyIndex> {
        return BinaryScale.decodeFromByteArray<List<SelectedVoucherKeyScale>>(stored.fromHex())
            .map { CoinageKeyIndex(CoinageInstallationId(it.installation), it.item) }
    }
}

@Serializable
private class SelectedVoucherKeyScale(
    val installation: DataByteArray,
    val item: Int,
)
