package io.paritytech.polkadotapp.common.presentation.sharing

import android.content.Intent
import android.provider.CalendarContract
import androidx.core.app.ShareCompat
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.ContentSharing
import io.paritytech.polkadotapp.common.utils.EventCalendarSharing
import javax.inject.Inject
import javax.inject.Singleton

interface SharingManager {
    fun shareContent(sharing: ContentSharing)
    fun shareText(text: String)
    fun shareCalendarEvent(sharing: EventCalendarSharing)
}

@Singleton
class RealSharingManager @Inject constructor(
    private val contextManager: ContextManager
) : SharingManager {
    override fun shareContent(sharing: ContentSharing) {
        val activity = contextManager.requireActivity()

        // startChooser rather than startActivity: a bare ACTION_SEND lets the system remember a default
        // target, which silently routes later shares into an app that cannot open the attachment.
        ShareCompat.IntentBuilder(activity)
            .setStream(sharing.file.uri)
            .setType(sharing.file.mimeType)
            .setSubject(sharing.subject)
            .setText(sharing.text)
            .apply {
                sharing.to?.let { addEmailTo(it) }
            }
            .startChooser()
    }

    override fun shareText(text: String) {
        val activity = contextManager.requireActivity()

        val sendIntent = ShareCompat.IntentBuilder(activity)
            .setType("text/plain")
            .setText(text)
            .intent

        activity.startActivity(Intent.createChooser(sendIntent, null))
    }

    override fun shareCalendarEvent(sharing: EventCalendarSharing) {
        val intent = Intent(Intent.ACTION_INSERT).apply {
            data = CalendarContract.Events.CONTENT_URI

            putExtra(CalendarContract.Events.TITLE, sharing.title)
            putExtra(CalendarContract.EXTRA_EVENT_BEGIN_TIME, sharing.startTime)
            putExtra(CalendarContract.EXTRA_EVENT_END_TIME, sharing.endTime)
            putExtra(CalendarContract.Events.ALL_DAY, false)
        }

        val activity = contextManager.requireActivity()
        activity.startActivity(intent)
    }
}
