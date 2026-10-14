package io.paritytech.polkadotapp.tools_media_connection_impl.turn

import io.paritytech.polkadotapp.tools_jwt_auth_api.CallWithBearerToken
import retrofit2.http.Body
import retrofit2.http.POST

internal interface TurnApi {
    @CallWithBearerToken
    @POST("api/v1/turn/issue")
    suspend fun issueTurnCredentials(@Body body: TurnIssueRequestBody): TurnCredentialsResponse
}
