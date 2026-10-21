package io.paritytech.polkadotapp.common.utils.calendar

import android.Manifest
import android.content.ContentProviderOperation
import android.content.ContentValues
import android.content.Context
import android.content.pm.PackageManager
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.CalendarContract
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.common.utils.EventCalendarSharing
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.isGranted
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.withContext
import timber.log.Timber
import java.util.TimeZone
import javax.inject.Inject
import kotlin.time.Duration

interface CalendarEventsMixin {
    fun observeEventAddedToCalendar(event: CalendarEvent): Flow<Boolean>

    suspend fun addEvent(event: CalendarEvent): Result<Unit>

    /** Adds [event] with an alert [alertBefore] its start, without prompting; a held event is left alone. */
    suspend fun addEventIfPermitted(event: CalendarEvent, alertBefore: Duration): Result<Unit>
}

class RealCalendarEventsMixin @Inject constructor(
    @ApplicationContext val context: Context,
    private val sharingManager: SharingManager,
    private val permissionAsker: PermissionAsker
) : CalendarEventsMixin {
    private val manualRefresh = MutableSharedFlow<Unit>(extraBufferCapacity = 1)

    override fun observeEventAddedToCalendar(event: CalendarEvent): Flow<Boolean> {
        return merge(androidCalendarChangedFlow(), manualRefresh)
            .conflate()
            .debounce(500L) // Short debounce to avoid calendar sync issue. May trigger a lot otherwise
            .map { isAddedToCalendar(event) }
            // Emit immediately on (re)subscribe so re-entering the chat reflects the persisted added-state.
            .onStart { emit(isAddedToCalendar(event)) }
            .flowOn(Dispatchers.IO)
            .catch { emit(false) }
            .distinctUntilChanged()
    }

    private fun isAddedToCalendar(event: CalendarEvent): Boolean {
        return readPermissionsAreGranted() && isEventAddedToCalendar(event)
    }

    override suspend fun addEvent(event: CalendarEvent): Result<Unit> = runCatching {
        val permissionResult = permissionAsker.askPermission(Manifest.permission.READ_CALENDAR, Manifest.permission.WRITE_CALENDAR)

        if (permissionResult.isGranted()) {
            if (isEventAddedToCalendar(event)) return@runCatching

            val calendarId = getMostRelevantCalendarId()
            val isCalendarIdFound = calendarId != null
            if (isCalendarIdFound) {
                addEventToCalendarDirectly(calendarId, event)
                manualRefresh.tryEmit(Unit)
            } else {
                runCalendarIntent(event)
            }
        } else {
            runCalendarIntent(event)
        }
    }

    private fun readPermissionsAreGranted() = permissionAsker.getPermissionState(Manifest.permission.READ_CALENDAR).isGranted()

    // Not PermissionAsker: its state read needs an activity, and a reminder can be scheduled with none open.
    private fun readWritePermissionsAreGranted() =
        listOf(Manifest.permission.READ_CALENDAR, Manifest.permission.WRITE_CALENDAR)
            .all { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }

    override suspend fun addEventIfPermitted(event: CalendarEvent, alertBefore: Duration): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                if (!readWritePermissionsAreGranted() || findEventId(event) != null) return@runCatching
                val calendarId = getMostRelevantCalendarId() ?: return@runCatching
                insertEventWithAlert(calendarId, event, alertBefore)
                manualRefresh.tryEmit(Unit)
            }
        }

    // One batch, so an event never lands without its alert.
    private fun insertEventWithAlert(calendarId: Long, event: CalendarEvent, alertBefore: Duration) {
        val operations = arrayListOf(
            ContentProviderOperation.newInsert(CalendarContract.Events.CONTENT_URI)
                .withValues(eventValues(calendarId, event))
                .build(),
            ContentProviderOperation.newInsert(CalendarContract.Reminders.CONTENT_URI)
                .withValueBackReference(CalendarContract.Reminders.EVENT_ID, 0)
                .withValue(CalendarContract.Reminders.MINUTES, alertBefore.inWholeMinutes)
                .withValue(CalendarContract.Reminders.METHOD, CalendarContract.Reminders.METHOD_ALERT)
                .build(),
        )
        context.contentResolver.applyBatch(CalendarContract.AUTHORITY, operations)
    }

    private fun addEventToCalendarDirectly(calendarId: Long, event: CalendarEvent) {
        context.contentResolver.insert(CalendarContract.Events.CONTENT_URI, eventValues(calendarId, event))
    }

    private fun eventValues(calendarId: Long, event: CalendarEvent) = ContentValues().apply {
        put(CalendarContract.Events.DTSTART, event.timeStart)
        put(CalendarContract.Events.DTEND, event.timeStart + event.duration.inWholeMilliseconds)
        put(CalendarContract.Events.TITLE, event.title)
        put(CalendarContract.Events.CALENDAR_ID, calendarId)
        put(CalendarContract.Events.EVENT_TIMEZONE, TimeZone.getDefault().id)
    }

    // As fallback we add event to calendar via intent
    private fun runCalendarIntent(event: CalendarEvent) {
        sharingManager.shareCalendarEvent(
            EventCalendarSharing(
                title = event.title,
                startTime = event.timeStart,
                endTime = event.timeStart + event.duration.inWholeMilliseconds
            )
        )
    }

    /**
     * We try to take calendar id by next rules:
     * - We are looking for google calendar id. Event if it's not primary we try to write there
     * - As a fallback we try to get any primary calendar. Local one for example
     *
     * We have to take google even if it's not primary since google doesn't support calendars from local storage. (But other calendars like Samsung may support it)
     */
    @Throws(SecurityException::class)
    private fun getMostRelevantCalendarId(): Long? {
        val projection = arrayOf(
            CalendarContract.Calendars._ID,
            CalendarContract.Calendars.ACCOUNT_TYPE,
            CalendarContract.Calendars.IS_PRIMARY,
            CalendarContract.Calendars.CALENDAR_ACCESS_LEVEL
        )

        // Lookup for visible only calendars. filter by access level to don't write into read only calendars
        val selection = "${CalendarContract.Calendars.VISIBLE} = 1 AND " +
            "${CalendarContract.Calendars.CALENDAR_ACCESS_LEVEL} >= ${CalendarContract.Calendars.CAL_ACCESS_CONTRIBUTOR}"

        val cursor = context.contentResolver.query(
            CalendarContract.Calendars.CONTENT_URI,
            projection,
            selection,
            null,
            null
        )

        cursor?.use {
            var googleId: Long? = null
            var primaryId: Long? = null

            while (it.moveToNext()) {
                val id = it.getLong(0)
                val accountType = it.getString(1)
                val isPrimary = it.getString(2) == "1"

                if (isPrimary) {
                    primaryId = id
                }

                if (accountType == "com.google") {
                    if (isPrimary) {
                        return id
                    }

                    if (googleId == null) {
                        googleId = id
                    }
                }
            }

            if (googleId != null) return googleId

            if (primaryId != null) return primaryId
        }

        return null
    }

    private fun isEventAddedToCalendar(event: CalendarEvent): Boolean {
        val eventId = runCatching { findEventId(event) }.getOrNull()
        return eventId != null
    }

    /**
     * Looking any event in android calendars that matches time start and title
     */
    @Throws(SecurityException::class)
    private fun findEventId(event: CalendarEvent): Long? {
        val selection = "${CalendarContract.Events.DTSTART} = ? AND ${CalendarContract.Events.TITLE} = ?"
        val selectionArgs = arrayOf(event.timeStart.toString(), event.title)

        try {
            context.contentResolver.query(
                CalendarContract.Events.CONTENT_URI,
                arrayOf(CalendarContract.Events._ID),
                selection,
                selectionArgs,
                null
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val id = cursor.getLong(0)
                    return id
                }
            }
        } catch (e: Exception) {
            Timber.e(e, "Failed to find event in calendar")
        }

        return null
    }

    /**
     * A flow that triggers when android system changes any calendar data
     */
    @Throws(SecurityException::class)
    private fun androidCalendarChangedFlow(): Flow<Unit> = callbackFlow {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) {
                trySend(Unit)
            }
        }

        runCatching {
            context.contentResolver.registerContentObserver(
                CalendarContract.Events.CONTENT_URI,
                true,
                observer
            )
        }

        trySend(Unit)

        awaitClose {
            runCatching {
                context.contentResolver.unregisterContentObserver(observer)
            }
        }
    }
}
