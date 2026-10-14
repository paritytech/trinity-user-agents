package io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api

import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.GetAttesterResponse
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.QueueStatusResponse
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.SearchUsernameBaseResponse
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameAvailableRequest
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameAvailableResponse
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameClaimRequest
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model.UsernameClaimResponse
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.CallWithWidevineIntegrity
import io.paritytech.polkadotapp.tools_jwt_auth_api.CallWithBearerToken
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Query

interface UsernameApi {
    @POST("api/v1/usernames")
    @CallWithBearerToken
    @CallWithWidevineIntegrity
    suspend fun claimUsername(@Body body: UsernameClaimRequest): UsernameClaimResponse

    @POST("api/v1/usernames/available")
    @CallWithBearerToken
    suspend fun checkUsername(
        @Body body: UsernameAvailableRequest,
        @Query("version") version: String = "v1",
    ): UsernameAvailableResponse

    @GET("api/v1/attester")
    suspend fun getAttester(): GetAttesterResponse

    @GET("api/v1/registration/queue")
    @CallWithBearerToken
    suspend fun getRegistrationQueueStatus(): QueueStatusResponse

    @GET("api/v1/usernames/search?status=ASSIGNED")
    @CallWithBearerToken
    suspend fun searchUsernames(
        @Query("prefix") prefix: String,
    ): SearchUsernameBaseResponse
}
