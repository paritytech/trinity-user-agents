package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.baseExtras
import io.paritytech.polkadotapp.common.data.telemetry.baseTags
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.ProductId
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
    ) : GameReminderEvent

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

class GameReminderTelemetry @Inject internal constructor(
    private val timeProvider: TimeProvider,
    private val originProvider: HostEventOriginProvider,
    private val capture: HostEventCapture,
) {
    fun report(event: GameReminderEvent) {
        runCatching {
            capture.capture {
                event.toHostEvent(originProvider.current(), timeProvider.now().toEpochMilliseconds())
            }
        }.logFailure("Failed to capture game reminder event")
    }
}

private fun GameReminderEvent.eventName() = when (this) {
    is GameReminderEvent.Scheduled -> "host.reminder.scheduled"
    is GameReminderEvent.NotScheduled -> "host.reminder.not_scheduled"
    is GameReminderEvent.Fired -> "host.reminder.fired"
    is GameReminderEvent.Tapped -> "host.reminder.tapped"
    is GameReminderEvent.ProductOpened -> "host.reminder.product_opened"
    is GameReminderEvent.Cancelled -> "host.reminder.cancelled"
}

private fun GameReminderEvent.toHostEvent(origin: HostEventOrigin, nowMillis: Long): HostEvent {
    val event = this
    val tags = buildMap {
        putAll(origin.baseTags())
        put("reminder.product_id", event.productId.value)
        when (event) {
            is GameReminderEvent.Scheduled -> put("reminder.delivery", event.delivery.value)
            is GameReminderEvent.Fired -> {
                put("reminder.delivery", event.delivery.value)
                put("host.app_state", if (event.foreground) "active" else "background")
                put("reminder.still_scheduled", event.stillScheduled.toString())
            }
            is GameReminderEvent.NotScheduled -> put("reminder.skip_reason", event.reason.value)
            is GameReminderEvent.Tapped,
            is GameReminderEvent.ProductOpened,
            is GameReminderEvent.Cancelled -> Unit
        }
    }
    val extras = buildMap<String, Any> {
        putAll(origin.baseExtras())
        event.startsAtMillis?.let { startsAt ->
            put("reminder.starts_at_unix_ms", startsAt)
            put("reminder.delay_after_start_ms", nowMillis - startsAt)
        }
    }
    return HostEvent(name = event.eventName(), tags = tags, extras = extras)
}
