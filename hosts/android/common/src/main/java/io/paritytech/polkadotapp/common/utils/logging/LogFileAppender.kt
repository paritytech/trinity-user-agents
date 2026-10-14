package io.paritytech.polkadotapp.common.utils.logging

import android.annotation.SuppressLint
import android.util.Log
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileWriter
import java.io.IOException
import java.time.ZoneOffset
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter

@SuppressLint("LogNotTimber")
class LogFileAppender(
    coroutineScope: CoroutineScope,
    private val fallbackTag: String,
    private val logFile: File
) {
    private companion object {
        val LOG_TIMESTAMP_FORMAT: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss.SSS")

        val MAX_LOG_FILE_SIZE = 4.megabytes
        const val ROTATED_FILE_SUFFIX = ".1"
        const val SIZE_CHECK_EVERY_N_MESSAGES = 200
        const val INTERNAL_LOG_TAG = "LogFileAppender"
    }

    private val logChannel = Channel<String>(Channel.UNLIMITED)

    private var logWriter: FileWriter? = null

    private var messagesSinceSizeCheck = 0

    init {
        coroutineScope.launch {
            try {
                logWriter = FileWriter(logFile, true)
                listenForLogs()
            } catch (e: IOException) {
                // Closing makes the later trySend calls fail instead of queueing into a channel nobody
                // drains any more, which would grow for the life of the process on a device out of space.
                logChannel.close()
                Log.e(INTERNAL_LOG_TAG, "Cannot open ${logFile.name} for writing. Logger is disabled", e)
            }
        }
    }

    fun append(priority: Int, tag: String?, message: String, t: Throwable?) {
        val finalTag = tag ?: fallbackTag

        var fullMessage = "$finalTag ${priorityToString(priority)} $message"

        if (t != null) {
            fullMessage += "\n${Log.getStackTraceString(t)}"
        }

        logChannel.trySend(fullMessage)
    }

    private suspend fun listenForLogs() {
        try {
            for (message in logChannel) {
                try {
                    val logTimestamp = LOG_TIMESTAMP_FORMAT.format(ZonedDateTime.now(ZoneOffset.UTC))
                    logWriter?.append("$logTimestamp $message\n")
                    logWriter?.flush()
                    rotateIfNeeded()
                } catch (e: IOException) {
                    Log.e(INTERNAL_LOG_TAG, "Failed to write to ${logFile.name}", e)
                }
            }
        } finally {
            withContext(NonCancellable) {
                try {
                    logWriter?.close()
                    logWriter = null
                } catch (e: IOException) {
                    Log.e(INTERNAL_LOG_TAG, "Failed to close the writer for ${logFile.name}", e)
                }
            }
        }
    }

    // The log is append-only, so without rotation it grows until a diagnostics export built from it is too
    // large to share. Size is stat-ed in batches to keep the per-message cost off the hot path.
    private fun rotateIfNeeded() {
        if (++messagesSinceSizeCheck < SIZE_CHECK_EVERY_N_MESSAGES) return
        messagesSinceSizeCheck = 0

        if (logFile.length().bytes < MAX_LOG_FILE_SIZE) return

        val rotatedFile = File(logFile.parentFile, logFile.name + ROTATED_FILE_SUFFIX)

        logWriter?.close()
        logWriter = null

        rotatedFile.delete()
        logFile.renameTo(rotatedFile)

        logWriter = FileWriter(logFile, true)
    }

    private fun priorityToString(priority: Int): String = when (priority) {
        Log.VERBOSE -> "VERBOSE"
        Log.DEBUG -> "DEBUG"
        Log.INFO -> "INFO"
        Log.WARN -> "WARN"
        Log.ERROR -> "ERROR"
        Log.ASSERT -> "ASSERT"
        else -> "?"
    }
}
