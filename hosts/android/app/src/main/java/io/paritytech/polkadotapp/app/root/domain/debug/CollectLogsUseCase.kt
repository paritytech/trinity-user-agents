package io.paritytech.polkadotapp.app.root.domain.debug

import io.paritytech.polkadotapp.app.BuildConfig
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.common.utils.ContentSharing
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import javax.inject.Inject
import io.paritytech.polkadotapp.common.utils.logging.LoggerConstants as AppLoggerConstants

class CollectLogsUseCase @Inject constructor(
    private val fileProvider: FileProvider,
    private val sharingManager: SharingManager
) {
    suspend operator fun invoke(): Result<Unit> = runCatching {
        val zipFile = fileProvider.getFileInInternalCacheStorage("logs.zip")

        zipFile.delete()

        ZipOutputStream(FileOutputStream(zipFile)).use { zipOutputStream ->
            addLogFileToZip(AppLoggerConstants.LOGS_FILE_NAME, zipOutputStream)
            addLogFileToZip(AppLoggerConstants.COINAGE_LOGS_FILE_NAME, zipOutputStream)
//            TODO: reimplement new game logs and add them here
//            val gameLogsDir = fileProvider.getFileInScopedStorage(GameLoggerConstants.LOGS_DIR)
//            if (gameLogsDir.exists()) {
//                zipSubdirectory(gameLogsDir.path, "game_logs", gameLogsDir, zipOutputStream)
//            }
        }

        val date = DateTimeFormatter.ofPattern("dd/MM/yy").format(ZonedDateTime.now())

        sharingManager.shareContent(
            sharing = ContentSharing.file(
                subject = "$date - Android",
                to = BuildConfig.LOG_COLLECTION_EMAIL,
                uri = fileProvider.uriOf(zipFile),
                mimeType = "application/zip"
            )
        )
    }

    private fun addLogFileToZip(fileName: String, zos: ZipOutputStream) {
        val logFile = fileProvider.getFileInScopedStorage("${AppLoggerConstants.LOGS_DIR}/$fileName")
        if (logFile.exists()) {
            addFileToZip(fileName, logFile, zos)
        }
    }

    private fun addFileToZip(entryName: String, file: File, zos: ZipOutputStream) {
        FileInputStream(file).use { fis ->
            val zipEntry = ZipEntry(entryName)
            zos.putNextEntry(zipEntry)
            fis.copyTo(zos)
            zos.closeEntry()
        }
    }

    private fun zipSubdirectory(basePath: String, entryPrefix: String, currentFile: File, zos: ZipOutputStream) {
        if (currentFile.isDirectory) {
            currentFile.listFiles()?.forEach { file ->
                zipSubdirectory(basePath, entryPrefix, file, zos)
            }
        } else {
            val entryPath = entryPrefix + "/" + currentFile.path.substring(basePath.length + 1)
            addFileToZip(entryPath, currentFile, zos)
        }
    }
}
