package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

enum class GameStartAlarmOffset(val seconds: Int) {
    FIVE_SECONDS(5),
    TEN_SECONDS(10),
    FIFTEEN_SECONDS(15),
    TWENTY_SECONDS(20);

    companion object {
        val DEFAULT = TWENTY_SECONDS

        fun fromSeconds(seconds: Int): GameStartAlarmOffset {
            return entries.find { it.seconds == seconds } ?: DEFAULT
        }
    }
}
