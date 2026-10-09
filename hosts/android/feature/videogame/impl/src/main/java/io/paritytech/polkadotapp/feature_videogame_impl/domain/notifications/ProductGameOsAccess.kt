package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

// Each ask prompts only while the app is in the foreground; otherwise it reports what the OS already allows.
// Notifications are the one precondition: nothing reminds without them. The other two only degrade the reminder.
interface ProductGameOsAccess {
    suspend fun requestNotifications(): Result<Unit>

    suspend fun requestExactAlarms()

    suspend fun requestCalendar(): Boolean
}
