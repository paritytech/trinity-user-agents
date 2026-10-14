package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.api

import io.paritytech.polkadotapp.tools_integrity_api.interceptors.CallWithIntegrity
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JWTTokenResponse
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JwtRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.RefreshTokenRequest
import retrofit2.http.Body
import retrofit2.http.POST

interface AuthTokenApi {
    @POST("api/v1/auth/token")
    @CallWithIntegrity
    suspend fun fetchToken(@Body body: JwtRequest): JWTTokenResponse

    @POST("api/v1/auth/token/refresh")
    suspend fun refreshToken(@Body body: RefreshTokenRequest): JWTTokenResponse
}
