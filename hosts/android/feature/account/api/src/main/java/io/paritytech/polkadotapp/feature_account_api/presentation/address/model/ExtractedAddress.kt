package io.paritytech.polkadotapp.feature_account_api.presentation.address.model

import android.os.Parcelable
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import kotlinx.parcelize.Parcelize

data class ExtractedAddress(
    val display: String,
    val type: DisplayType,
    val accountId: AccountId,
) {
    enum class DisplayType {
        USERNAME, ADDRESS
    }
}

@Parcelize
class ExtractedAddressParcel(
    val display: String,
    val type: ExtractedAddress.DisplayType,
    val accountId: ByteArray,
) : Parcelable

fun ExtractedAddress.toParcel(): ExtractedAddressParcel {
    return ExtractedAddressParcel(display, type, accountId.value)
}

fun ExtractedAddressParcel.toDomain(): ExtractedAddress {
    return ExtractedAddress(display, type, accountId.intoAccountId())
}
