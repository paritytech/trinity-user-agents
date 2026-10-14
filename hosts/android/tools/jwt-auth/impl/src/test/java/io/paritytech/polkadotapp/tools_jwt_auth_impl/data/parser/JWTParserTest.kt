package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.parser

import com.google.gson.Gson
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.Base64

class JWTParserTest {
    private lateinit var parser: JWTParser

    @Before
    fun setUp() {
        parser = JWTParser(Gson())
    }

    // MARK: - Well-formed tokens

    @Test
    fun `parses JWT with integer exp`() {
        val token = makeJWT("""{"sub":"a","exp":1700000000}""")
        val payload = parser.parse(token).getOrThrow()

        assertEquals(1_700_000_000L, payload.exp)
    }

    @Test
    fun `parses iat and nbf claims`() {
        val token = makeJWT("""{"exp":2000,"iat":1000,"nbf":1500}""")
        val payload = parser.parse(token).getOrThrow()

        assertEquals(1000L, payload.iat)
        assertEquals(1500L, payload.nbf)
    }

    @Test
    fun `parses large exp value`() {
        val token = makeJWT("""{"exp":9999999999}""")
        val payload = parser.parse(token).getOrThrow()

        assertEquals(9_999_999_999L, payload.exp)
    }

    @Test
    fun `preserves extra claims in raw map`() {
        val token = makeJWT("""{"exp":1,"sub":"user-123","role":"admin"}""")
        val payload = parser.parse(token).getOrThrow()

        assertEquals("user-123", payload.claims["sub"])
        assertEquals("admin", payload.claims["role"])
    }

    @Test
    fun `missing exp claim returns null`() {
        val token = makeJWT("""{"sub":"a"}""")
        val payload = parser.parse(token).getOrThrow()

        assertNull(payload.exp)
    }

    // MARK: - Base64url handling

    @Test
    fun `handles payloads without padding`() {
        for (length in 1..4) {
            val json = """{"exp":${"1".repeat(length)}}"""
            val token = makeJWT(json, stripPadding = true)
            val payload = parser.parse(token).getOrThrow()
            assertNotNull(payload.exp)
        }
    }

    // MARK: - Malformed input

    @Test
    fun `fails on empty string`() {
        assertTrue(parser.parse("").exceptionOrNull() is JWTParsingException.InvalidFormat)
    }

    @Test
    fun `fails on single segment`() {
        assertTrue(parser.parse("only-one").exceptionOrNull() is JWTParsingException.InvalidFormat)
    }

    @Test
    fun `fails on two segments`() {
        assertTrue(parser.parse("only.two").exceptionOrNull() is JWTParsingException.InvalidFormat)
    }

    @Test
    fun `fails on four segments`() {
        assertTrue(parser.parse("a.b.c.d").exceptionOrNull() is JWTParsingException.InvalidFormat)
    }

    @Test
    fun `fails on invalid base64 payload`() {
        assertTrue(parser.parse("header.!!!not-base64!!!.sig").exceptionOrNull() is JWTParsingException.InvalidBase64)
    }

    @Test
    fun `fails on non-JSON payload`() {
        val payload = Base64.getUrlEncoder().withoutPadding()
            .encodeToString("hello world".toByteArray())
        assertTrue(parser.parse("header.$payload.sig").exceptionOrNull() is JWTParsingException.InvalidJSON)
    }

    @Test
    fun `fails on JSON array payload`() {
        val payload = Base64.getUrlEncoder().withoutPadding()
            .encodeToString("[1,2,3]".toByteArray())
        assertTrue(parser.parse("header.$payload.sig").exceptionOrNull() is JWTParsingException.InvalidJSON)
    }

    @Test
    fun `non-numeric exp is ignored`() {
        val token = makeJWT("""{"exp":"not-a-number"}""")
        val payload = parser.parse(token).getOrThrow()

        assertNull(payload.exp)
    }

    // MARK: - Helpers

    private fun makeJWT(payload: String, stripPadding: Boolean = false): String {
        val encoder = if (stripPadding) {
            Base64.getUrlEncoder().withoutPadding()
        } else {
            Base64.getUrlEncoder()
        }
        val header = encoder.encodeToString("""{"alg":"HS256"}""".toByteArray())
        val payloadEncoded = encoder.encodeToString(payload.toByteArray())
        return "$header.$payloadEncoded.sig"
    }
}
