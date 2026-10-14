package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.common.R as RCommon

enum class LinkedDeviceCategory(@StringRes val titleRes: Int) {
    LAPTOP_DESKTOP(RCommon.string.linked_devices_category_laptop_desktop)
}
