package io.paritytech.polkadotapp.app.root.data.debug

import io.paritytech.polkadotapp.app.BuildConfig
import io.paritytech.polkadotapp.app.root.domain.debug.IssueReport
import io.paritytech.polkadotapp.app.root.domain.debug.IssueReportApi
import io.paritytech.polkadotapp.app.root.domain.debug.IssueReportSubmissionError
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import io.paritytech.polkadotapp.common.utils.await
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.withContext
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.asRequestBody
import okio.BufferedSink
import java.util.concurrent.TimeUnit
import javax.inject.Inject

/** Uses synchronized Firebase configuration without request logging or automatic replay. */
class RealIssueReportApi @Inject constructor(
    private val remoteConfig: RemoteConfigService,
    builder: OkHttpClient.Builder,
    private val dispatchers: CoroutineDispatchers,
) : IssueReportApi {
    private val client = builder
        .apply {
            interceptors().clear()
            networkInterceptors().clear()
        }
        .cache(null)
        .retryOnConnectionFailure(false)
        .followRedirects(false)
        .followSslRedirects(false)
        .connectTimeout(20, TimeUnit.SECONDS)
        .writeTimeout(5, TimeUnit.MINUTES)
        .readTimeout(5, TimeUnit.MINUTES)
        .callTimeout(5, TimeUnit.MINUTES)
        .build()

    override suspend fun send(report: IssueReport): Result<Unit> = withContext(dispatchers.io) {
        val endpoint = remoteConfig.getSyncedString("issue_proxy_url")
            .getOrElse { return@withContext Result.failure(it) }
        val configuredKey = remoteConfig.getSyncedString("issue_proxy_api_key")
            .getOrElse { return@withContext Result.failure(it) }
        val url = endpoint.trim().toHttpUrlOrNull()
        val key = configuredKey.trim()
        if (url == null || !url.isHttps || url.username.isNotEmpty() || url.password.isNotEmpty() ||
            url.fragment != null || key.isEmpty() || key.any { it.code !in 33..126 }
        ) {
            return@withContext Result.failure(IssueReportSubmissionError.NotConfigured)
        }

        runCancellableCatching {
            val description = report.description + "\n\n" + """
                ## App information

                - App ID: `${BuildConfig.APPLICATION_ID}`
                - App version: `${BuildConfig.VERSION_NAME}`
                - Build: `${BuildConfig.VERSION_CODE}`
            """.trimIndent()
            val body = MultipartBody.Builder().setType(MultipartBody.FORM)
                .addFormDataPart("title", "Android app issue")
                .addFormDataPart("body", description)
                .addFormDataPart("screenshot", "screenshot.png", report.screenshot.asRequestBody("image/png".toMediaType()))
                .addFormDataPart("logs", "logs.zip", report.logs.asRequestBody("application/zip".toMediaType()))
                .build()
            if (report.screenshot.length().bytes > 10.megabytes || body.contentLength().bytes > 25.megabytes) {
                return@withContext Result.failure(IssueReportSubmissionError.AttachmentsTooLarge)
            }
            val request = Request.Builder()
                .url(url)
                .header("Authorization", "Bearer $key")
                .post(object : RequestBody() {
                    override fun contentType() = body.contentType()
                    override fun contentLength() = body.contentLength()
                    override fun writeTo(sink: BufferedSink) = body.writeTo(sink)
                    override fun isOneShot() = true
                })
                .build()
            client.newCall(request).await().use { response ->
                if (response.code != 201) {
                    return@withContext Result.failure(IssueReportSubmissionError.HttpFailure(response.code))
                }
            }
        }
    }
}
