package io.paritytech.polkadotapp.feature_videogame_impl.data.notifications

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ScheduledProductGame
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.serialization.json.Json
import javax.inject.Inject
import javax.inject.Singleton

private const val KEY_PRODUCT_GAME_SLOTS = "video_game_product_game_slots"

@Singleton
class VideoGameSettingsPreferences @Inject constructor(
    private val preferences: Preferences
) {
    fun getScheduledProductGames(): List<ScheduledProductGame> = preferences.getString(KEY_PRODUCT_GAME_SLOTS).toScheduledGames()

    fun scheduledProductGamesFlow(): Flow<List<ScheduledProductGame>> = preferences.stringFlow(KEY_PRODUCT_GAME_SLOTS)
        .map { it.toScheduledGames() }
        .distinctUntilChanged()

    fun setScheduledProductGames(games: List<ScheduledProductGame>) {
        preferences.putString(KEY_PRODUCT_GAME_SLOTS, games.takeIf { it.isNotEmpty() }?.let(Json::encodeToString))
    }

    private fun String?.toScheduledGames(): List<ScheduledProductGame> =
        this?.let { runCatching { Json.decodeFromString<List<ScheduledProductGame>>(it) }.getOrNull() } ?: emptyList()
}
