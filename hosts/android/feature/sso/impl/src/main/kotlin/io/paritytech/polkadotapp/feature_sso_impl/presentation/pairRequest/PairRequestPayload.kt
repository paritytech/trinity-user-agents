package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest

import android.os.Parcelable
import kotlinx.parcelize.Parcelize

@Parcelize
class PairRequestPayload(
    val statementAccountId: ByteArray,
    val encryptionPublicKey: ByteArray,
    val metadata: Map<MetadataKey, String>,
) : Parcelable

sealed interface MetadataKey : Parcelable {
    @Parcelize
    data class Custom(val name: String) : MetadataKey

    @Parcelize
    data object HostName : MetadataKey

    @Parcelize
    data object HostVersion : MetadataKey

    @Parcelize
    data object HostIcon : MetadataKey

    @Parcelize
    data object PlatformType : MetadataKey

    @Parcelize
    data object PlatformVersion : MetadataKey

    @Parcelize
    data object Location : MetadataKey
}
