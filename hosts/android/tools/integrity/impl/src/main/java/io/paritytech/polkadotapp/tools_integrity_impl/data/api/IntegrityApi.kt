package io.paritytech.polkadotapp.tools_integrity_impl.data.api

import io.paritytech.polkadotapp.tools_integrity_impl.data.model.ChallengeResponse
import retrofit2.http.POST

interface IntegrityApi {
    @POST("api/v1/auth/challenges")
    suspend fun fetchChallenge(): ChallengeResponse
}
