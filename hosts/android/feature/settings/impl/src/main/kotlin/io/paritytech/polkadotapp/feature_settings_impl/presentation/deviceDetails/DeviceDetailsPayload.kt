package io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails

import android.os.Parcelable
import kotlinx.parcelize.Parcelize

@Parcelize
data class DeviceDetailsPayload(
    val deviceId: String,
) : Parcelable
