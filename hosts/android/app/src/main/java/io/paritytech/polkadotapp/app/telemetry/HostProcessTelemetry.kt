package io.paritytech.polkadotapp.app.telemetry

import android.os.Build
import android.os.SystemClock
import io.paritytech.polkadotapp.app.BuildConfig
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.protocol.Message
import uniffi.truapi.truapiHostVersion
import java.util.UUID

internal data class HostProcessMetadata(
    val appVersion: String,
    val appBuild: Long,
    val appVariant: String,
    val platform: String,
    val osVersion: String,
    val truapiHostVersion: String,
)

internal data class HostProcessStartEvent(
    val name: String,
    val appLaunchId: String,
    val monotonicMs: Long,
    val metadata: HostProcessMetadata,
)

internal data class HostProcessSentryFields(
    val tags: Map<String, String>,
    val extras: Map<String, Any>,
)

internal fun HostProcessStartEvent.sentryFields() = HostProcessSentryFields(
    tags = mapOf(
        "app.launch_id" to appLaunchId,
        "host.native_app_version" to metadata.appVersion,
        "host.app_variant" to metadata.appVariant,
        "host.platform" to metadata.platform,
        "host.os_version" to metadata.osVersion,
        "truapi.host_version" to metadata.truapiHostVersion,
    ),
    extras = mapOf(
        "host.app_build" to metadata.appBuild,
        "event.monotonic_ms" to monotonicMs,
    ),
)

internal class HostProcessTelemetry(
    private val appLaunchId: String,
    private val monotonicMillis: () -> Long,
    private val metadata: () -> HostProcessMetadata,
    private val capture: (HostProcessStartEvent) -> Unit,
) {
    fun captureProcessStarted(enabled: Boolean) {
        if (!enabled) return

        runCatching {
            capture(
                HostProcessStartEvent(
                    name = PROCESS_STARTED_EVENT,
                    appLaunchId = appLaunchId,
                    monotonicMs = monotonicMillis(),
                    metadata = metadata(),
                )
            )
        }
    }

    private companion object {
        const val PROCESS_STARTED_EVENT = "host.process.started"
    }
}

internal object AndroidHostProcessTelemetry {
    private val telemetry = HostProcessTelemetry(
        appLaunchId = UUID.randomUUID().toString(),
        monotonicMillis = SystemClock::elapsedRealtime,
        metadata = {
            HostProcessMetadata(
                appVersion = BuildConfig.VERSION_NAME,
                appBuild = BuildConfig.VERSION_CODE.toLong(),
                appVariant = BuildConfig.BUILD_TYPE,
                platform = "android",
                osVersion = Build.VERSION.RELEASE,
                truapiHostVersion = truapiHostVersion(),
            )
        },
        capture = ::captureWithSentry,
    )

    fun captureProcessStarted() {
        telemetry.captureProcessStarted(
            enabled = BuildConfig.BUILD_TYPE == "debug" ||
                BuildConfig.BUILD_TYPE == "nightly" ||
                BuildConfig.BUILD_TYPE == "safetynet"
        )
    }

    private fun captureWithSentry(event: HostProcessStartEvent) {
        val sentryEvent = SentryEvent().apply {
            message = Message().apply { formatted = event.name }
            level = SentryLevel.INFO
            logger = "host.telemetry"
            val fields = event.sentryFields()
            for ((key, value) in fields.tags) setTag(key, value)
            for ((key, value) in fields.extras) setExtra(key, value)
        }
        Sentry.captureEvent(sentryEvent)
    }
}
