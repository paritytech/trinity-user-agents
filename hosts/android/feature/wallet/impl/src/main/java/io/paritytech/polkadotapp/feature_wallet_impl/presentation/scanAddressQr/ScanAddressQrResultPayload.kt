package io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr

import android.os.Parcelable
import kotlinx.parcelize.Parcelize

@Parcelize
data class ScanAddressQrResultPayload(
    val address: String
) : Parcelable
