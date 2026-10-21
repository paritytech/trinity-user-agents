package io.paritytech.polkadotapp.app.telemetry

import android.os.Build
import android.os.Process
import io.paritytech.polkadotapp.app.BuildConfig
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.parity.truapi.BuildConfig as TrUAPIBuildConfig
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.protocol.Message
import java.util.UUID
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

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
    val appLaunchId: UUID,
    val processStartElapsedRealtime: Duration,
    val metadata: HostProcessMetadata,
)

internal data class HostProcessSentryFields(
    val tags: Map<String, String>,
    val extras: Map<String, Any>,
)

internal fun HostProcessStartEvent.sentryFields() = HostProcessSentryFields(
    tags = mapOf(
        "host.native_app_version" to metadata.appVersion,
        "host.app_variant" to metadata.appVariant,
        "host.platform" to metadata.platform,
        "host.os_version" to metadata.osVersion,
        "truapi.host_version" to metadata.truapiHostVersion,
    ),
    extras = mapOf(
        "app.launch_id" to appLaunchId.toString(),
        "host.app_build" to metadata.appBuild,
        "event.monotonic_ms" to processStartElapsedRealtime.inWholeMilliseconds,
    ),
)

internal fun appVariant(flavor: String, buildType: String): String =
    "$flavor${buildType.replaceFirstChar { it.titlecase() }}"

internal inline fun captureHostProcessStartedIfEnabled(
    enabled: Boolean,
    capture: () -> Result<Unit>,
): Result<Unit> = if (enabled) capture() else Result.success(Unit)

internal class HostProcessTelemetry(
    private val appLaunchId: UUID,
    private val processStartElapsedRealtime: () -> Duration,
    private val metadata: () -> HostProcessMetadata,
    private val capture: (HostProcessStartEvent) -> Unit,
) {
    fun captureProcessStarted(): Result<Unit> = runCatching {
        capture(
            HostProcessStartEvent(
                name = PROCESS_STARTED_EVENT,
                appLaunchId = appLaunchId,
                processStartElapsedRealtime = processStartElapsedRealtime(),
                metadata = metadata(),
            )
        )
    }

    private companion object {
        const val PROCESS_STARTED_EVENT = "host.process.started"
    }
}

internal class AndroidHostProcessTelemetryInitializer @Inject constructor() : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = captureHostProcessStartedIfEnabled(
        enabled = FeatureOption.HOST_PROCESS_TELEMETRY.isEnabled,
    ) {
        HostProcessTelemetry(
            appLaunchId = UUID.randomUUID(),
            processStartElapsedRealtime = { Process.getStartElapsedRealtime().milliseconds },
            metadata = {
                HostProcessMetadata(
                    appVersion = BuildConfig.VERSION_NAME,
                    appBuild = BuildConfig.VERSION_CODE.toLong(),
                    appVariant = appVariant(BuildConfig.FLAVOR, BuildConfig.BUILD_TYPE),
                    platform = "android",
                    osVersion = Build.VERSION.RELEASE,
                    truapiHostVersion = TrUAPIBuildConfig.TRUAPI_HOST_VERSION,
                )
            },
            capture = ::captureWithSentry,
        ).captureProcessStarted()
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
