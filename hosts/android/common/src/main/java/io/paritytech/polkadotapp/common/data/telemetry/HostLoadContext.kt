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

    fun tags(): Map<String, String> = mapOf("load.trigger" to trigger.tag)
}

/**
 * Which loads were already reported. The WebView asks for its product again on every request it serves,
 * and inside a session one report per step and name says all of them: the read, or the first cache hit.
 * Outside a session, once per trigger.
 */
@Singleton
class HostCacheHits @Inject constructor() {
    private val reported = ConcurrentHashMap<String, MutableSet<String>>()

    fun isFirst(event: String, name: String, load: HostLoadContext): Boolean =
        reported.computeIfAbsent(load.sessionId.orEmpty()) { ConcurrentHashMap.newKeySet() }
            .add("$event|$name|${load.trigger.tag}")

    /** Drops what [sessionId] reported, once nothing will load under it again. */
    fun forget(sessionId: String) {
        reported.remove(sessionId)
    }
}
