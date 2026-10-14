package io.paritytech.polkadotapp.app.root.domain.debug

import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import io.paritytech.polkadotapp.common.utils.logging.LoggerConstants
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File
import java.util.UUID
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import javax.inject.Inject

class CollectIssueLogsUseCase @Inject constructor(
    private val fileProvider: FileProvider,
    private val coroutineDispatchers: CoroutineDispatchers
) {
    suspend operator fun invoke(): Result<File> {
        var archive: File? = null
        var succeeded = false
        try {
            val result = withContext(coroutineDispatchers.io) {
                runCancellableCatching {
                    val logFile = fileProvider.getFileInScopedStorage("${LoggerConstants.LOGS_DIR}/${LoggerConstants.LOGS_FILE_NAME}")
                    if (!logFile.isFile || logFile.length() == 0L) {
                        return@withContext Result.failure(DebugLogError.MissingLogs)
                    }

                    val zipFile = fileProvider.getFileInInternalCacheStorage("polkadot-logs-${UUID.randomUUID()}.zip")
                    archive = zipFile
                    writeArchive(logFile, zipFile)
                    if (zipFile.length().bytes > MAX_ARCHIVE_SIZE) {
                        return@withContext Result.failure(DebugLogError.ArchiveTooLarge)
                    }
                    zipFile
                }
            }
            succeeded = result.isSuccess
            return result
        } finally {
            if (!succeeded) {
                withContext(NonCancellable + coroutineDispatchers.io) {
                    archive?.delete()
                }
            }
        }
    }

    private suspend fun writeArchive(logFile: File, zipFile: File) {
        ZipOutputStream(zipFile.outputStream()).use { output ->
            output.putNextEntry(ZipEntry(LoggerConstants.LOGS_FILE_NAME))
            logFile.inputStream().use { input ->
                val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                while (true) {
                    currentCoroutineContext().ensureActive()
                    val bytesRead = input.read(buffer)
                    if (bytesRead == -1) break
                    output.write(buffer, 0, bytesRead)
                }
            }
            output.closeEntry()
        }
    }

    private companion object {
        val MAX_ARCHIVE_SIZE = 25.megabytes
    }
}
