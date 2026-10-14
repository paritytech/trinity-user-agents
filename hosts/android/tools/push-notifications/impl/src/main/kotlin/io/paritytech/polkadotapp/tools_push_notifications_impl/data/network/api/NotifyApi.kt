package io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.api

import io.paritytech.polkadotapp.tools_jwt_auth_api.CallWithBearerToken
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.NotifyRequest
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.NotifyResponse
import retrofit2.http.Body
import retrofit2.http.POST

interface NotifyApi {
    @POST("api/v1/notify")
    @CallWithBearerToken
    suspend fun notify(@Body body: NotifyRequest): NotifyResponse
}
