package io.paritytech.polkadotapp.common.utils

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.Intent.ACTION_SEND
import android.content.Intent.ACTION_SENDTO
import android.content.Intent.ACTION_VIEW
import android.content.Intent.EXTRA_EMAIL
import android.content.Intent.EXTRA_SUBJECT
import android.content.Intent.EXTRA_TEXT
import android.content.Intent.FLAG_ACTIVITY_NEW_TASK
import android.content.Intent.FLAG_ACTIVITY_REQUIRE_NON_BROWSER
import android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION
import android.content.Intent.createChooser
import android.net.Uri
import android.os.Build
import android.widget.Toast
import io.paritytech.polkadotapp.common.R

const val SYSTEM_EMAIL_ACTION_PREFIX = "mailto:"

fun Context.browseUrl(url: String): Boolean {
    val fixedUrl = when {
        url.startsWith("https://") -> url
        url.startsWith("http://") -> url.replace("http://", "https://")
        else -> "https://$url"
    }
    val intent = Intent(ACTION_VIEW).apply {
        data = Uri.parse(fixedUrl)
    }
    return tryStartActivity(intent)
}

fun Context.openPdf(uri: Uri): Boolean {
    val intent = Intent(ACTION_VIEW).apply {
        setFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or FLAG_ACTIVITY_NEW_TASK or FLAG_GRANT_READ_URI_PERMISSION)
        setDataAndType(uri, "application/pdf")
    }

    return tryStartActivity(intent)
}

fun Context.openImage(uri: Uri): Boolean {
    val intent = Intent(ACTION_VIEW).apply {
        setFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or FLAG_ACTIVITY_NEW_TASK or FLAG_GRANT_READ_URI_PERMISSION)
        setDataAndType(uri, "image/*")
    }

    return tryStartActivity(intent)
}

fun Context.openEmail(
    email: String,
    subject: String? = null,
    body: String? = null
): Boolean {
    val selectorIntent = Intent(ACTION_SENDTO)
    selectorIntent.data = Uri.parse(SYSTEM_EMAIL_ACTION_PREFIX)

    val emailIntent = Intent(ACTION_SEND).apply {
        data = Uri.parse(SYSTEM_EMAIL_ACTION_PREFIX)
        putExtra(EXTRA_EMAIL, arrayOf(email))
        subject?.let {
            putExtra(EXTRA_SUBJECT, subject)
        }
        body?.let {
            putExtra(EXTRA_TEXT, body)
        }
        selector = selectorIntent
    }

    val intent = createChooser(emailIntent, null)

    return tryStartActivity(intent)
}

fun Context.openUri(uri: Uri): Boolean {
    val intent = Intent(ACTION_VIEW).apply {
        data = uri
        // Callers pass non-activity contexts (@ApplicationContext in
        // SpaBrowserViewModel / RealSpaHost). The safetyStart helpers only
        // add NEW_TASK on SDK >= R, so on API 29 (minSdk) startActivity
        // throws AndroidRuntimeException — which is NOT one of the caught
        // exception types. Set it unconditionally, like openPdf/openImage.
        addFlags(FLAG_ACTIVITY_NEW_TASK)
    }
    return tryStartActivity(intent)
}

private fun Context.tryStartActivity(intent: Intent) =
    safetyStartNativeActivity(intent) || safetyStartActivity(intent)
        .also { success ->
            if (success.not()) {
                Toast.makeText(this, getString(R.string.app_not_found_error), Toast.LENGTH_SHORT).show()
            }
        }

private fun Context.safetyStartNativeActivity(intent: Intent): Boolean =
    try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            intent.flags = intent.flags + FLAG_ACTIVITY_REQUIRE_NON_BROWSER or FLAG_ACTIVITY_NEW_TASK
        }
        startActivity(intent)
        true
    } catch (e: ActivityNotFoundException) {
        false
    } catch (e: SecurityException) {
        false
    }

private fun Context.safetyStartActivity(intent: Intent): Boolean =
    try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            intent.removeFlags(FLAG_ACTIVITY_REQUIRE_NON_BROWSER)
        }
        startActivity(intent)
        true
    } catch (e: ActivityNotFoundException) {
        false
    } catch (e: SecurityException) {
        false
    }
