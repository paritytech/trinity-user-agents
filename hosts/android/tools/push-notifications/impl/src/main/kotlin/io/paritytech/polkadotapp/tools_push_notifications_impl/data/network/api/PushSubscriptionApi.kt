package io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.api

import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.AddRulesResponse
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.RemoveRulesResponse
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.ReplaceRulesResponse
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.RulesRequest
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.SubscriptionRequest
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.models.SubscriptionResponse
import retrofit2.http.Body
import retrofit2.http.DELETE
import retrofit2.http.GET
import retrofit2.http.HTTP
import retrofit2.http.POST
import retrofit2.http.PUT

interface PushSubscriptionApi {
    @POST("api/v1/subscriptions")
    suspend fun register(@Body body: SubscriptionRequest): SubscriptionResponse

    @DELETE("api/v1/subscriptions")
    suspend fun delete()

    @GET("api/v1/subscriptions")
    suspend fun get(): SubscriptionResponse

    @PUT("api/v1/subscriptions/rules")
    suspend fun replaceRules(@Body body: RulesRequest): ReplaceRulesResponse

    @POST("api/v1/subscriptions/rules")
    suspend fun addRules(@Body body: RulesRequest): AddRulesResponse

    @HTTP(method = "DELETE", path = "api/v1/subscriptions/rules", hasBody = true)
    suspend fun removeRules(@Body body: RulesRequest): RemoveRulesResponse
}
