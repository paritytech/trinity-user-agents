package io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models

import androidx.annotation.Keep

@Keep
class NotifyRequest(
    val deviceToken: String,
    val pushId: String,
    val bundlerId: String,
    val platform: String,
    val message: String,
    val voip: Boolean
)
