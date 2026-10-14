package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.parser

import com.google.gson.Gson
import com.google.gson.JsonSyntaxException
import java.util.Base64
import javax.inject.Inject

sealed class JWTParsingException(message: String) : Exception(message) {
    class InvalidFormat : JWTParsingException("Token does not have exactly three dot-separated segments")
    class InvalidBase64 : JWTParsingException("Payload segment is not valid base64url")
    class InvalidJSON : JWTParsingException("Decoded payload is not a JSON object")
}

data class JWTPayload(
    val exp: Long?,
    val iat: Long?,
    val nbf: Long?,
    val claims: Map<String, Any?>
)

class JWTParser @Inject constructor(
    private val gson: Gson,
) {
    fun parse(token: String): Result<JWTPayload> = runCatching {
        val segments = token.split(".")
        if (segments.size != 3) throw JWTParsingException.InvalidFormat()

        val payloadBytes = decodeBase64Url(segments[1])
        val claims = parseJsonObject(payloadBytes)

        JWTPayload(
            exp = claims.toLongOrNull("exp"),
            iat = claims.toLongOrNull("iat"),
            nbf = claims.toLongOrNull("nbf"),
            claims = claims
        )
    }

    private fun decodeBase64Url(segment: String): ByteArray = try {
        Base64.getUrlDecoder().decode(segment)
    } catch (_: IllegalArgumentException) {
        throw JWTParsingException.InvalidBase64()
    }

    @Suppress("UNCHECKED_CAST")
    private fun parseJsonObject(bytes: ByteArray): Map<String, Any?> = try {
        gson.fromJson(String(bytes, Charsets.UTF_8), Map::class.java) as? Map<String, Any?>
            ?: throw JWTParsingException.InvalidJSON()
    } catch (_: JsonSyntaxException) {
        throw JWTParsingException.InvalidJSON()
    }

    private fun Map<String, Any?>.toLongOrNull(key: String): Long? {
        val value = get(key) ?: return null
        return when (value) {
            is Number -> value.toLong()
            else -> null
        }
    }
}
