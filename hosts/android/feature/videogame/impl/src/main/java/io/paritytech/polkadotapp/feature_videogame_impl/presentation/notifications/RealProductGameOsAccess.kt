package io.paritytech.polkadotapp.feature_videogame_impl.presentation.notifications

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.net.toUri
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.utils.ActivityResultExecutor
import io.paritytech.polkadotapp.common.utils.canScheduleExactAlarms
import io.paritytech.polkadotapp.common.utils.permissions.ForegroundPrompt
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.PermissionResult
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ProductGameOsAccess
import javax.inject.Inject

private const val KEY_EXACT_ALARM_REFUSED = "product_exact_alarm_access_refused"

// The game product needs no per-product consent, so the reminder asks the OS directly.
class RealProductGameOsAccess @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val permissionAsker: PermissionAsker,
    private val foregroundPrompt: ForegroundPrompt,
    private val preferences: Preferences,
) : ProductGameOsAccess {
    override suspend fun requestNotifications(): Result<Unit> {
        val allowed = NotificationManagerCompat.from(context).areNotificationsEnabled() ||
            (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && ask(Manifest.permission.POST_NOTIFICATIONS))
        return if (allowed) Result.success(Unit) else Result.failure(IllegalStateException("notifications are not allowed"))
    }

    // A refusal is remembered until the user allows it in system settings, so it is not asked again.
    override suspend fun requestExactAlarms() {
        if (context.canScheduleExactAlarms()) return
        if (preferences.getBoolean(KEY_EXACT_ALARM_REFUSED, false)) return
        val allowed = foregroundPrompt.ask { activity -> ExactAlarmAccessExecutor(activity).execute().getOrNull() } ?: return
        preferences.putBoolean(KEY_EXACT_ALARM_REFUSED, !allowed)
    }

    // READ as well: deduping an added event queries the calendar.
    override suspend fun requestCalendar(): Boolean = ask(Manifest.permission.READ_CALENDAR, Manifest.permission.WRITE_CALENDAR)

    private suspend fun ask(vararg permissions: String): Boolean {
        if (permissions.all { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }) {
            return true
        }
        return foregroundPrompt.ask { permissionAsker.askPermission(*permissions) == PermissionResult.GRANTED } == true
    }
}

// The settings screen returns CANCELED whatever the user chose, so the answer is read back from the OS.
private class ExactAlarmAccessExecutor(
    private val activity: ComponentActivity,
) : ActivityResultExecutor<Boolean>(activity) {
    override fun createIntent() =
        Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, "package:${activity.packageName}".toUri())

    override fun handleResult(result: ActivityResult) = Result.success(activity.canScheduleExactAlarms())
}
