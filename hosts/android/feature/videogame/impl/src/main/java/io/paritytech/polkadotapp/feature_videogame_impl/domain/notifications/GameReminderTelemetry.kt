package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

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

sealed interface GameReminderEvent {
    val productId: ProductId
    val startsAtMillis: Long?

    data class Scheduled(
        override val productId: ProductId,
        override val startsAtMillis: Long,
        val delivery: GameReminderDelivery,
    ) : GameReminderEvent

    data class NotScheduled(
        override val productId: ProductId,
        override val startsAtMillis: Long,
        val reason: GameReminderSkipReason,
    ) : GameReminderEvent

    data class Fired(
        override val productId: ProductId,
        override val startsAtMillis: Long?,
        val foreground: Boolean,
        val stillScheduled: Boolean,
        val delivery: GameReminderDelivery,
    ) : GameReminderEvent

    data class Tapped(override val productId: ProductId, override val startsAtMillis: Long?) : GameReminderEvent

    data class ProductOpened(
        override val productId: ProductId,
        override val startsAtMillis: Long?,
        val source: GameReminderOpenSource,
    ) : GameReminderEvent

    data class Missed(override val productId: ProductId, override val startsAtMillis: Long) : GameReminderEvent

    data class Cancelled(override val productId: ProductId, override val startsAtMillis: Long) : GameReminderEvent
}

enum class GameReminderDelivery(val value: String) {
    ALARM("alarm"),
    NOTIFICATION("notification"),
}

enum class GameReminderSkipReason(val value: String) {
    NOT_AUTHORIZED("not_authorized"),
    START_IN_PAST("start_in_past"),
    FAILED("failed"),
}

enum class GameReminderOpenSource(val value: String) {
    AUTO_OPEN("auto_open"),
    ALARM("alarm"),
}

fun interface GameReminderEventCapture {
    fun capture(event: SentryEvent)
}

class SentryGameReminderEventCapture @Inject constructor() : GameReminderEventCapture {
    override fun capture(event: SentryEvent) {
        if (!FeatureOption.HOST_TELEMETRY.isEnabled) return

        Sentry.captureEvent(event)
    }
}

class GameReminderTelemetry @Inject constructor(
    private val timeProvider: TimeProvider,
    private val capture: GameReminderEventCapture,
) {
    fun report(event: GameReminderEvent) {
        runCatching { capture.capture(event.toSentryEvent(timeProvider.now().toEpochMilliseconds())) }
            .logFailure("Failed to capture game reminder event")
    }
}

private fun GameReminderEvent.eventName() = when (this) {
    is GameReminderEvent.Scheduled -> "host.reminder.scheduled"
    is GameReminderEvent.NotScheduled -> "host.reminder.not_scheduled"
    is GameReminderEvent.Fired -> "host.reminder.fired"
    is GameReminderEvent.Tapped -> "host.reminder.tapped"
    is GameReminderEvent.ProductOpened -> "host.reminder.product_opened"
    is GameReminderEvent.Missed -> "host.reminder.missed"
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
        setExtra("reminder.delay_after_start_ms", nowMillis - startsAt)
    }
    when (event) {
        is GameReminderEvent.Scheduled -> setTag("reminder.delivery", event.delivery.value)
        is GameReminderEvent.Fired -> {
            setTag("reminder.delivery", event.delivery.value)
            setTag("host.app_state", if (event.foreground) "active" else "background")
            setTag("reminder.still_scheduled", event.stillScheduled.toString())
        }
        is GameReminderEvent.ProductOpened -> setTag("reminder.open_source", event.source.value)
        is GameReminderEvent.NotScheduled -> setTag("reminder.skip_reason", event.reason.value)
        is GameReminderEvent.Tapped,
        is GameReminderEvent.Missed,
        is GameReminderEvent.Cancelled -> Unit
    }
}
