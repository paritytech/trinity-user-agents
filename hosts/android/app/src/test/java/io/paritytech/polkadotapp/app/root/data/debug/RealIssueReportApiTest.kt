package io.paritytech.polkadotapp.app.root.data.debug

import io.paritytech.polkadotapp.app.BuildConfig
import io.paritytech.polkadotapp.app.root.domain.debug.IssueReport
import io.paritytech.polkadotapp.app.root.domain.debug.IssueReportSubmissionError
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.test_shared.whenever
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import okhttp3.tls.HandshakeCertificates
import okhttp3.tls.HeldCertificate
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.mockito.Mockito.mock
import java.util.concurrent.TimeUnit

class RealIssueReportApiTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val server = MockWebServer()
    private val config = mock(RemoteConfigService::class.java)
    private val dispatchers = mock(CoroutineDispatchers::class.java)
    private lateinit var sender: RealIssueReportApi
    private lateinit var report: IssueReport

    @Before
    fun setUp() = runBlocking<Unit> {
        val certificate = HeldCertificate.Builder().addSubjectAlternativeName("localhost").build()
        val serverCertificates = HandshakeCertificates.Builder().heldCertificate(certificate).build()
        val clientCertificates = HandshakeCertificates.Builder().addTrustedCertificate(certificate.certificate).build()
        server.useHttps(serverCertificates.sslSocketFactory())
        server.start()
        whenever(dispatchers.io).thenReturn(Dispatchers.IO)
        whenever(config.getSyncedString("issue_proxy_url")).thenReturn(Result.success(server.url("/v1/issues").toString()))
        whenever(config.getSyncedString("issue_proxy_api_key")).thenReturn(Result.success("app-key"))
        val builder = OkHttpClient.Builder()
            .sslSocketFactory(clientCertificates.sslSocketFactory(), clientCertificates.trustManager)
            .addInterceptor { error("Reports must not reach shared request loggers") }
        sender = RealIssueReportApi(config, builder, dispatchers)
        report = IssueReport(
            "The screen froze 🐛",
            temporaryFolder.newFile("logs.zip").apply { writeBytes(byteArrayOf(0x50, 0x4b, 0x03, 0x04)) },
            temporaryFolder.newFile("screenshot.png").apply { writeBytes(byteArrayOf(0x89.toByte(), 0x50, 0x4e, 0x47)) },
        )
    }

    @After
    fun tearDown() = server.close()

    @Test
    fun `uploads description and unchanged attachments with proxy authentication`() = runBlocking<Unit> {
        server.enqueue(MockResponse.Builder().code(201).body("{\"number\":1,\"url\":\"https://example.com/1\"}").build())

        assertEquals(Result.success(Unit), sender.send(report))

        val request = requireNotNull(server.takeRequest(5, TimeUnit.SECONDS))
        val boundary = requireNotNull(request.headers["Content-Type"]).substringAfter("boundary=")
        val expectedBody = Buffer()
        fun append(name: String, bytes: ByteArray, attachment: String = "") {
            expectedBody.writeUtf8("--$boundary\r\nContent-Disposition: form-data; name=\"$name\"$attachment\r\n")
            expectedBody.writeUtf8("\r\n")
            expectedBody.write(bytes).writeUtf8("\r\n")
        }
        append("title", "Android app issue".toByteArray())
        val expectedDescription = report.description + "\n\n## App information\n\n" +
            "- App ID: `${BuildConfig.APPLICATION_ID}`\n" +
            "- App version: `${BuildConfig.VERSION_NAME}`\n" +
            "- Build: `${BuildConfig.VERSION_CODE}`"
        append("body", expectedDescription.toByteArray())
        append("screenshot", report.screenshot.readBytes(), "; filename=\"screenshot.png\"\r\nContent-Type: image/png")
        append("logs", report.logs.readBytes(), "; filename=\"logs.zip\"\r\nContent-Type: application/zip")
        expectedBody.writeUtf8("--$boundary--\r\n")
        assertEquals(listOf("POST", "/v1/issues", "Bearer app-key"), listOf(request.method, request.target, request.headers["Authorization"]))
        assertEquals(expectedBody.readByteString(), request.body)
    }

    @Test
    fun `fails promptly without networking for missing or unsafe configuration`() = runBlocking<Unit> {
        val endpoint = server.url("/v1/issues").toString()
        for ((url, key) in listOf("" to "app-key", endpoint to "", "http://localhost/v1/issues" to "app-key", endpoint to "key\r\nX-Other: value")) {
            whenever(config.getSyncedString("issue_proxy_url")).thenReturn(Result.success(url))
            whenever(config.getSyncedString("issue_proxy_api_key")).thenReturn(Result.success(key))
            assertEquals(IssueReportSubmissionError.NotConfigured, withTimeout(1_000) { sender.send(report) }.exceptionOrNull())
        }
        assertEquals(0, server.requestCount)
    }

    @Test
    fun `only accepts created and never follows redirects or retries rejection`() = runBlocking<Unit> {
        for (status in listOf(200, 202, 307, 401, 413, 503)) {
            server.enqueue(MockResponse.Builder().code(status)
                .addHeader("Location", server.url("/redirect"))
                .addHeader("Retry-After", "0").build())
            server.enqueue(MockResponse.Builder().code(201).build())
            assertEquals(IssueReportSubmissionError.HttpFailure(status), sender.send(report).exceptionOrNull())
            assertEquals(Result.success(Unit), sender.send(report))
        }
        assertEquals(12, server.requestCount)
    }
}
