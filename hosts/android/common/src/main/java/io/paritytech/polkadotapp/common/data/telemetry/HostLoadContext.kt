package io.paritytech.polkadotapp.common.data.telemetry

import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext

/** Why a product load ran: for a page or worker being opened, or ahead of one. */
enum class HostLoadTrigger(val tag: String) {
    OPEN("open"),
    PREWARM("prewarm"),
}

/**
 * Names the trigger and host session of the product loads run under it, read when a load starts. A
 * load run outside one reports nothing: icons, chat names and face images read the same records, and
 * counting them would charge an open with work it never waited on. A prewarm carries no session, since
 * it runs ahead of any page.
 */
class HostLoadContext(
    val trigger: HostLoadTrigger,
    val sessionId: String?,
) : AbstractCoroutineContextElement(Key) {
    companion object Key : CoroutineContext.Key<HostLoadContext>

    fun tags(): Map<String, String> = mapOf("trigger" to trigger.tag)
}

/**
 * Which loads answered from memory were already reported. The WebView asks for its product again on
 * every request it serves, and one hit per trigger and session says all of them.
 */
@Singleton
class HostCacheHits @Inject constructor() {
    private val reported = ConcurrentHashMap.newKeySet<String>()

    fun isFirst(event: String, name: String, load: HostLoadContext): Boolean =
        reported.add("$event|$name|${load.trigger.tag}|${load.sessionId.orEmpty()}")
}
