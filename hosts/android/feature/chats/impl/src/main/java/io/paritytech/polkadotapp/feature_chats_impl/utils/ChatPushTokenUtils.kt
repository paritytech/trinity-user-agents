package io.paritytech.polkadotapp.feature_chats_impl.utils

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushToken

object ChatPushTokenUtils {
    fun createAndroidToken(value: String): ChatPushToken {
        return ChatPushToken(value.toByteArray(Charsets.UTF_8))
    }

    fun getPlatformToken(chatPushToken: ChatPushToken, operatingSystem: OperatingSystem): String = when (operatingSystem) {
        OperatingSystem.UNKNOWN -> error("Operating system should be known to get push token value")
        OperatingSystem.ANDROID -> chatPushToken.value.toString(Charsets.UTF_8)
        OperatingSystem.IOS -> chatPushToken.value.toHexString(withPrefix = false)
    }
}
