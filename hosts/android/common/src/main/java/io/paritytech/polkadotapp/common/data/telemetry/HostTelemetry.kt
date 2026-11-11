package io.paritytech.polkadotapp.common.data.telemetry

import android.content.Intent
import io.paritytech.polkadotapp.common.utils.logFailure
import kotlinx.coroutines.TimeoutCancellationException
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.cancellation.CancellationException

data class HostEventOrigin(
    val appVariant: String,
    val appFlavor: String,
    val nativeAppVersion: String,
    val platform: String,
    val osVersion: String,
    val appBuild: Long,
)

data class HostEvent(
    val name: String,
    val tags: Map<String, String>,
    val extras: Map<String, Any>,
    val isFailure: Boolean = false,
)

const val PRODUCT_NOTIFICATION_INTENT_MARKER = "product_notification"
const val PRODUCT_NOTIFICATION_INTENT_PRODUCT_ID = "product_notification_product_id"

fun HostEventOrigin.baseTags(): Map<String, String> = mapOf(
    "host.platform" to platform,
    "host.app_variant" to appVariant,
    "host.app_flavor" to appFlavor,
    "host.native_app_version" to nativeAppVersion,
)

fun HostEventOrigin.baseExtras(): Map<String, Any> = mapOf("host.app_build" to appBuild)

fun HostEventOrigin.productNotificationShown(productId: String) = HostEvent(
    name = "host.product_notification.shown",
    tags = baseTags() + ("product_notification.product_id" to productId),
    extras = baseExtras(),
)

fun HostEventOrigin.productNotificationTapped(productId: String) = HostEvent(
    name = "host.product_notification.tapped",
    tags = baseTags() + ("product_notification.product_id" to productId),
    extras = baseExtras(),
)

fun HostEventOrigin.pocketOpened(open: HostPageOpen, reused: Boolean) = HostEvent(
    name = "host.pocket.opened",
    tags = baseTags() + mapOf(
        "open.source" to open.source.tag,
        "host.session_id" to open.sessionId,
        "open.reused" to reused.toString(),
    ),
    extras = baseExtras(),
)

enum class HostLoadStage(val tag: String) {
    /** The page's own session: runtime, execution, WebView, first load. */
    PAGE("page"),
    WORKER("worker"),
    RESOLVE("resolve"),
    BULLETIN("bulletin"),
}

fun HostEventOrigin.loadFailed(
    stage: HostLoadStage,
    cause: Throwable,
    productId: String?,
    sessionId: String?,
    domain: String?,
) = HostEvent(
    name = "host.load.failed",
    tags = baseTags() + listOfNotNull(
        "load.stage" to stage.tag,
        "error.type" to cause.javaClass.simpleName,
        productId?.let { "product.id" to it },
        domain?.let { "dotns.domain" to it },
        sessionId?.let { "host.session_id" to it },
    ),
    extras = baseExtras() + ("error.message" to cause.message.orEmpty()),
    isFailure = true,
)

/** A tap is recorded once: not again when the activity is restored or relaunched from Recents. */
fun isFreshNotificationLaunch(isRestored: Boolean, intentFlags: Int): Boolean =
    !isRestored && (intentFlags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) == 0

fun interface HostEventOriginProvider {
    fun current(): HostEventOrigin
}

fun interface HostEventCapture {
    fun capture(event: () -> HostEvent)
}

fun interface HostMonotonicTimeProvider {
    fun nowMillis(): Long
}

/**
 * Reports a product load that failed, as one event, so Sentry can count and group the failures by stage.
 * Resolution is retried on every request and by background warm-ups, so a failure is reported once per
 * stage, product and error type for the life of the app, not once per attempt.
 */
@Singleton
class HostLoadFailureTelemetry @Inject constructor(
    private val originProvider: HostEventOriginProvider,
    private val capture: HostEventCapture,
) {
    private val reported = ConcurrentHashMap.newKeySet<List<String?>>()

    fun report(
        stage: HostLoadStage,
        cause: Throwable,
        productId: String? = null,
        sessionId: String? = null,
        domain: String? = null,
    ) {
        // A page or worker closed mid-load is not a failure; a timeout is.
        if (cause is CancellationException && cause !is TimeoutCancellationException) return
        if (!reported.add(listOf(stage.tag, productId, domain, cause.javaClass.name))) return
        runCatching {
            capture.capture { originProvider.current().loadFailed(stage, cause, productId, sessionId, domain) }
        }.logFailure("Failed to capture native host load failure")
    }
}
