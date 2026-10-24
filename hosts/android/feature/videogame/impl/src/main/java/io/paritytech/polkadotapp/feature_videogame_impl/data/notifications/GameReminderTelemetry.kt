package io.paritytech.polkadotapp.feature_videogame_impl.data.notifications

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import io.sentry.protocol.Message
import javax.inject.Inject

// Names and fields match the iOS host's host.reminder.* events. The alarm rings at the game's start, so the
// start is also the scheduled fire time.
sealed interface GameReminderEvent {
    val productId: ProductId
    val startsAtMillis: Long?

    data class Scheduled(override val productId: ProductId, override val startsAtMillis: Long, val exact: Boolean) : GameReminderEvent

    data class NotScheduled(override val productId: ProductId, override val startsAtMillis: Long) : GameReminderEvent

    data class ExactAlarmDenied(override val productId: ProductId, override val startsAtMillis: Long) : GameReminderEvent

    data class Fired(
        override val productId: ProductId,
        override val startsAtMillis: Long?,
        val foreground: Boolean,
        val stillScheduled: Boolean,
    ) : GameReminderEvent

    data class Dismissed(override val productId: ProductId, override val startsAtMillis: Long?) : GameReminderEvent

    data class Tapped(override val productId: ProductId, override val startsAtMillis: Long?) : GameReminderEvent

    data class ProductOpened(
        override val productId: ProductId,
        override val startsAtMillis: Long?,
        val source: GameReminderOpenSource,
    ) : GameReminderEvent

    data class Cancelled(override val productId: ProductId, override val startsAtMillis: Long) : GameReminderEvent
}

enum class GameReminderOpenSource(val value: String) {
    AUTO_OPEN("auto_open"),
    ALARM("alarm"),
}

class GameReminderTelemetry private constructor(
    private val enabled: () -> Boolean,
    private val nowMillis: () -> Long,
    private val capture: (SentryEvent) -> Unit,
) {
    @Inject
    constructor(timeProvider: TimeProvider) : this(
        enabled = { FeatureOption.HOST_TELEMETRY.isEnabled },
        nowMillis = { timeProvider.now().toEpochMilliseconds() },
        capture = { Sentry.captureEvent(it) },
    )

    fun report(event: GameReminderEvent) {
        if (!enabled()) return

        runCatching { capture(event.toSentryEvent(nowMillis())) }
            .logFailure("Failed to capture game reminder event")
    }

    companion object {
        internal fun createForTest(
            enabled: () -> Boolean,
            nowMillis: () -> Long,
            capture: (SentryEvent) -> Unit,
        ) = GameReminderTelemetry(enabled, nowMillis, capture)
    }
}

private fun GameReminderEvent.eventName() = when (this) {
    is GameReminderEvent.Scheduled -> "host.reminder.scheduled"
    is GameReminderEvent.NotScheduled -> "host.reminder.not_scheduled"
    is GameReminderEvent.ExactAlarmDenied -> "host.reminder.exact_alarm_denied"
    is GameReminderEvent.Fired -> "host.reminder.fired"
    is GameReminderEvent.Dismissed -> "host.reminder.dismissed"
    is GameReminderEvent.Tapped -> "host.reminder.tapped"
    is GameReminderEvent.ProductOpened -> "host.reminder.product_opened"
    is GameReminderEvent.Cancelled -> "host.reminder.cancelled"
}

private fun GameReminderEvent.toSentryEvent(nowMillis: Long) = SentryEvent().apply {
    val event = this@toSentryEvent
    message = Message().apply { formatted = event.eventName() }
    level = SentryLevel.INFO
    logger = "host.telemetry"
    setTag("host.platform", "android")
    setTag("reminder.product_id", event.productId.value)
    event.startsAtMillis?.let { startsAt ->
        setExtra("reminder.starts_at_unix_ms", startsAt)
        // Wall clock: negative while the alarm is still ahead, the lateness once it has fired.
        setExtra("reminder.delay_after_start_ms", nowMillis - startsAt)
    }
    when (event) {
        is GameReminderEvent.Scheduled -> setTag("reminder.exact", event.exact.toString())
        is GameReminderEvent.Fired -> {
            setTag("host.app_state", if (event.foreground) "active" else "background")
            setTag("reminder.still_scheduled", event.stillScheduled.toString())
        }
        is GameReminderEvent.ProductOpened -> setTag("reminder.open_source", event.source.value)
        is GameReminderEvent.NotScheduled -> setTag("reminder.skip_reason", "not_authorized")
        is GameReminderEvent.ExactAlarmDenied,
        is GameReminderEvent.Dismissed,
        is GameReminderEvent.Tapped,
        is GameReminderEvent.Cancelled -> Unit
    }
}
