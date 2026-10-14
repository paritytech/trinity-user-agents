package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model

import com.google.gson.Gson
import com.google.gson.JsonSyntaxException
import org.junit.Assert.assertEquals
import org.junit.Test

class JWTTokenResponseTest {
    private val gson = Gson()

    @Test
    fun `decodes valid response with token and refreshToken`() {
        val json = """{"token": "eyJhbGciOiJIUzI1NiJ9.test.sig", "refreshToken": "abc123def456"}"""
        val response = gson.fromJson(json, JWTTokenResponse::class.java)

        assertEquals("eyJhbGciOiJIUzI1NiJ9.test.sig", response.token)
        assertEquals("abc123def456", response.refreshToken)
    }

    @Test(expected = JsonSyntaxException::class)
    fun `fails on invalid JSON`() {
        gson.fromJson("not json", JWTTokenResponse::class.java)
    }

    @Test
    fun `missing fields decode as null`() {
        val json = """{}"""
        val response = gson.fromJson(json, JWTTokenResponse::class.java)

        // Gson sets missing String fields to null rather than throwing
        assertEquals(null, response.token)
        assertEquals(null, response.refreshToken)
    }

    @Test
    fun `RefreshTokenRequest serializes correctly`() {
        val request = RefreshTokenRequest(refreshToken = "a1b2c3d4")
        val json = gson.toJson(request)
        val parsed = gson.fromJson(json, Map::class.java)

        assertEquals("a1b2c3d4", parsed["refreshToken"])
    }

    @Test
    fun `RefreshTokenRequest round-trip`() {
        val original = RefreshTokenRequest(refreshToken = "test-refresh-token")
        val json = gson.toJson(original)
        val decoded = gson.fromJson(json, RefreshTokenRequest::class.java)

        assertEquals(original, decoded)
    }
}
