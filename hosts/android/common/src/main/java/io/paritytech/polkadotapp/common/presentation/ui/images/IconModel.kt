package io.paritytech.polkadotapp.common.presentation.ui.images

import androidx.annotation.DrawableRes

sealed interface IconModel {
    data class FromDrawableRes(@DrawableRes val res: Int) : IconModel
}
