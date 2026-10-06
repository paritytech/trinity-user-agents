package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.VideoGameSettingsPreferences
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.Serializable
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.seconds

// A game start a product asked to be reminded of; [ringAlarm] says whether exact alarms were allowed at schedule time.
@Serializable
data class ScheduledProductGame(val productId: String, val startsAtMillis: Long, val ringAlarm: Boolean)

private val PRODUCT_GAME_START_GRACE = 30.seconds

// Live until the grace after its start has run out.
internal fun ScheduledProductGame.isLiveAt(nowMillis: Long) =
    nowMillis - startsAtMillis < PRODUCT_GAME_START_GRACE.inWholeMilliseconds

@Singleton
class RealProductGameReminder @Inject constructor(
    private val preferences: VideoGameSettingsPreferences,
    private val scheduler: VideoGameReminderScheduler,
    private val notificationPublisher: VideoGameNotificationPublisher,
    private val calendar: ProductGameCalendar,
    private val osAccess: ProductGameOsAccess,
    private val timeProvider: TimeProvider,
) : ProductGameReminder {
    private val lock = Mutex()

    // Applies schedules and cancels in call order across their OS prompts; [lock] guards only the
    // scheduled games, so their readers never wait on a prompt.
    private val requestLock = Mutex()

    val scheduled: Flow<List<ScheduledProductGame>> get() = preferences.scheduledProductGamesFlow()

    fun currentlyScheduled(): List<ScheduledProductGame> = preferences.getScheduledProductGames()

    fun scheduledFor(productId: ProductId): ScheduledProductGame? =
        currentlyScheduled().firstOrNull { it.productId == productId.value }

    // An alarm needs both notifications and exact alarms; without exact alarms a notification still reminds.
    override suspend fun schedule(productId: ProductId, startsAtMillis: Long): Result<Unit> = requestLock.withLock {
        val leadMillis = startsAtMillis - now()
        osAccess.requestNotifications().map {
            val ringAlarm = osAccess.requestExactAlarms()
            lock.withLock { add(ScheduledProductGame(productId.value, startsAtMillis, ringAlarm)) }
            if (leadMillis >= CALENDAR_MIN_LEAD.inWholeMilliseconds && osAccess.requestCalendar()) {
                calendar.addGame(startsAtMillis)
            }
        }
    }

    override suspend fun cancel(productId: ProductId) {
        requestLock.withLock {
            lock.withLock { scheduledFor(productId)?.let(::remove) }
        }
    }

    override suspend fun restore() {
        lock.withLock {
            val now = now()
            val (stale, live) = currentlyScheduled().partition { !it.isLiveAt(now) }
            stale.forEach(::remove)
            live.forEach { scheduler.scheduleProductGameStart(it.product(), it.startsAtMillis) }
        }
    }

    // Only removes [game] if still scheduled, so a schedule made meanwhile survives.
    suspend fun clear(game: ScheduledProductGame) {
        lock.withLock { remove(game) }
    }

    private fun add(game: ScheduledProductGame) {
        save(currentlyScheduled().filterNot { it.productId == game.productId } + game)
        notificationPublisher.cancelProductGameStartsSoonNotification(game.product())
        scheduler.scheduleProductGameStart(game.product(), game.startsAtMillis)
    }

    private fun remove(game: ScheduledProductGame) {
        val scheduled = currentlyScheduled()
        if (game !in scheduled) return
        save(scheduled - game)
        scheduler.cancelProductGameStart(game.product())
        notificationPublisher.cancelProductGameStartsSoonNotification(game.product())
    }

    private fun save(games: List<ScheduledProductGame>) =
        preferences.setScheduledProductGames(games.sortedBy { it.startsAtMillis })

    private fun now() = timeProvider.now().toEpochMilliseconds()

    private companion object {
        val CALENDAR_MIN_LEAD = 1.hours
    }
}

fun ScheduledProductGame.product(): ProductId = ProductId.fromStoredValue(productId)
