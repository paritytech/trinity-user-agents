package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

interface VideoGameReminderScheduler {
    fun scheduleProductGameStart(productId: ProductId, gameStartMillis: Timestamp)
    fun cancelProductGameStart(productId: ProductId)
}

class RealVideoGameReminderScheduler @Inject constructor(
    private val alarms: GameReminderAlarms,
    private val timeProvider: TimeProvider,
    private val telemetry: GameReminderTelemetry,
) : VideoGameReminderScheduler {
    override fun scheduleProductGameStart(productId: ProductId, gameStartMillis: Timestamp) {
        if (gameStartMillis <= timeProvider.now().toEpochMilliseconds()) {
            alarms.cancel(productId)
            telemetry.report(GameReminderEvent.NotScheduled(productId, gameStartMillis, GameReminderSkipReason.START_IN_PAST))
            return
        }

        val exact = alarms.canRingExactly()
        try {
            alarms.set(productId, gameStartMillis, exact = exact)
        } catch (e: SecurityException) {
            telemetry.report(GameReminderEvent.NotScheduled(productId, gameStartMillis, GameReminderSkipReason.FAILED))
            throw e
        }
        val delivery = if (exact) GameReminderDelivery.ALARM else GameReminderDelivery.NOTIFICATION
        telemetry.report(GameReminderEvent.Scheduled(productId, gameStartMillis, delivery))
    }

    override fun cancelProductGameStart(productId: ProductId) = alarms.cancel(productId)
}
