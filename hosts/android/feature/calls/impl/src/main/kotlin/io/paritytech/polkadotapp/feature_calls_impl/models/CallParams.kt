package io.paritytech.polkadotapp.feature_calls_impl.models

import android.os.Parcelable
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import kotlinx.parcelize.Parcelize

@Parcelize
class CallParams(
    val chatId: ByteArray,
    val offerId: OfferId,
    val callerName: String,
    val direction: CallDirection,
    val withVideo: Boolean,
) : Parcelable
