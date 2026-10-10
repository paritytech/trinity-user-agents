package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

interface ProductGameCalendar {
    suspend fun addGame(startsAtMillis: Long)
}
