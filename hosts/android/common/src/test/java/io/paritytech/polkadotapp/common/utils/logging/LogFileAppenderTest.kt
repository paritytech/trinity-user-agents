package io.paritytech.polkadotapp.common.utils.logging

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class LogFileAppenderTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    private val scopes = mutableListOf<CoroutineScope>()

    @After
    fun closeAppenders() {
        scopes.forEach { it.cancel() }
    }

    @Test
    fun `writes the tag priority and message of an appended line`() {
        val logFile = temporaryFolder.newFile(LOG_FILE_NAME)

        appenderOn(logFile).append(Log.INFO, "Ledger", "submit-transaction", null)

        assertTrue(logFile.readLines().single().endsWith("Ledger INFO submit-transaction"))
    }

    @Test
    fun `labels a line that carries no tag with the fallback tag`() {
        val logFile = temporaryFolder.newFile(LOG_FILE_NAME)

        appenderOn(logFile).append(Log.DEBUG, null, "untagged", null)

        assertTrue(logFile.readLines().single().endsWith("$FALLBACK_TAG DEBUG untagged"))
    }

    @Test
    fun `keeps writing to the live file while it stays under the size limit`() {
        val logFile = temporaryFolder.newFile(LOG_FILE_NAME)
        val appender = appenderOn(logFile)

        repeat(MESSAGES_PER_SIZE_CHECK) { appender.append(Log.INFO, "Bulk", "short", null) }

        assertFalse(rotatedCounterpartOf(logFile).exists())
        assertEquals(MESSAGES_PER_SIZE_CHECK, logFile.readLines().size)
    }

    @Test
    fun `moves the live file aside once it passes the size limit`() {
        val logFile = temporaryFolder.newFile(LOG_FILE_NAME)
        val appender = appenderOn(logFile)
        val bulkyMessage = "x".repeat(BYTES_PER_MESSAGE)

        repeat(MESSAGES_PER_SIZE_CHECK) { appender.append(Log.INFO, "Bulk", bulkyMessage, null) }

        val rotatedFile = rotatedCounterpartOf(logFile)
        assertEquals(MESSAGES_PER_SIZE_CHECK, rotatedFile.readLines().size)
        assertTrue(rotatedFile.readLines().last().endsWith("Bulk INFO $bulkyMessage"))
    }

    @Test
    fun `keeps writing to a fresh live file after a rotation`() {
        val logFile = temporaryFolder.newFile(LOG_FILE_NAME)
        val appender = appenderOn(logFile)
        val bulkyMessage = "x".repeat(BYTES_PER_MESSAGE)
        repeat(MESSAGES_PER_SIZE_CHECK) { appender.append(Log.INFO, "Bulk", bulkyMessage, null) }

        appender.append(Log.INFO, "Bulk", "after-rotation", null)

        assertTrue(logFile.readLines().single().endsWith("Bulk INFO after-rotation"))
    }

    private fun rotatedCounterpartOf(logFile: File) = File(logFile.parentFile, logFile.name + ROTATED_SUFFIX)

    // Unconfined rather than the project's TestScope dispatchers: the appender drains its channel from a
    // consumer coroutine, and on a scheduled test dispatcher that consumer never runs, so nothing is written.
    private fun appenderOn(logFile: File): LogFileAppender {
        val scope = CoroutineScope(Dispatchers.Unconfined)
        scopes += scope

        return LogFileAppender(coroutineScope = scope, fallbackTag = FALLBACK_TAG, logFile = logFile)
    }

    private companion object {
        const val LOG_FILE_NAME = "logs.log"
        const val FALLBACK_TAG = "Fallback"
        const val ROTATED_SUFFIX = ".1"

        const val SIZE_LIMIT_BYTES = 4 * 1024 * 1024
        const val MESSAGES_PER_SIZE_CHECK = 200

        // The appender only measures the file every MESSAGES_PER_SIZE_CHECK lines, so a rotation has to be
        // provoked within one batch rather than by a long run of short lines.
        const val BYTES_PER_MESSAGE = SIZE_LIMIT_BYTES / MESSAGES_PER_SIZE_CHECK + 1
    }
}
