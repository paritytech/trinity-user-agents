package io.paritytech.polkadotapp.common.utils

import android.os.Parcelable
import androidx.core.os.bundleOf

fun Parcelable.toPayloadBundle(key: String = this::class.java.name) = bundleOf(key to this)
