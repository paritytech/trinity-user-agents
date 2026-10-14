package io.paritytech.polkadotapp.common.utils

import android.app.AlarmManager
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Size
import androidx.core.content.getSystemService
import androidx.core.net.toUri

fun Context.canScheduleExactAlarms(): Boolean {
    return Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
        getSystemService<AlarmManager>()?.canScheduleExactAlarms() == true
}

fun Context.openAppSettings() {
    val intent = Intent(
        Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
        Uri.fromParts("package", packageName, null)
    )

    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    startActivity(intent)
}

fun Context.openAppNotificationSettings() {
    val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).apply {
        putExtra(Settings.EXTRA_APP_PACKAGE, packageName)
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }

    startActivity(intent)
}

fun Context.openAppLanguageSettings() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
        openAppSettings()
        return
    }

    val intent = Intent(
        Settings.ACTION_APP_LOCALE_SETTINGS,
        Uri.fromParts("package", packageName, null)
    )

    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    runCatching { startActivity(intent) }.onFailure { openAppSettings() }
}

fun Context.getResourceUri(resId: Int): Uri {
    return "android.resource://$packageName/$resId".toUri()
}

fun Context.getResourceImageSize(resId: Int): Size {
    val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeResource(resources, resId, options)
    return Size(options.outWidth, options.outHeight)
}
