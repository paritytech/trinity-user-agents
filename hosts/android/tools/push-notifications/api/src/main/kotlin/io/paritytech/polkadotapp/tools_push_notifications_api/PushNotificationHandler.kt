package io.paritytech.polkadotapp.tools_push_notifications_api

interface PushNotificationHandler {
    suspend fun canHandle(data: Map<String, String>): Boolean
    suspend fun handle(data: Map<String, String>)
}
