package io.paritytech.polkadotapp.tools_push_notifications_api

interface PushNotificationHandler {
    /** A handler of a higher priority is asked first. */
    val priority: Int
        get() = 0

    suspend fun canHandle(data: Map<String, String>): Boolean
    suspend fun handle(data: Map<String, String>)
}
