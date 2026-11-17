package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.common.utils.ContentSharing
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logging.LoggerConstants
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.tokenAmount
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHolding
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ShareCoinageLogsUseCase
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.time.ZoneOffset
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import javax.inject.Inject

class RealShareCoinageLogsUseCase @Inject constructor(
    private val fileProvider: FileProvider,
    private val sharingManager: SharingManager,
    private val holdingsUseCase: CoinageHoldingsUseCase,
    private val dispatchers: CoroutineDispatchers
) : ShareCoinageLogsUseCase {
    private companion object {
        const val EXPORT_FILE_PREFIX = "coinage_logs_"
        const val EXPORT_MIME_TYPE = "application/zip"
        const val EXPORT_SUBJECT = "Coinage Diagnostics"
        const val REPORT_ENTRY_NAME = "coinage_report.txt"
        const val LOG_SNAPSHOT_NAME = "coinage_log_snapshot.log"
        const val CENTS_PER_DOLLAR = 100.0

        val FILE_NAME_TIMESTAMP_FORMAT: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss")
        val REPORT_TIMESTAMP_FORMAT: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss")
    }

    override suspend operator fun invoke(): Result<Unit> = runCatching {
        val exportFile = withContext(dispatchers.io) {
            writeExport(holdingsUseCase.subscribeHoldings().first().holdings)
        }

        sharingManager.shareContent(
            ContentSharing.file(
                subject = EXPORT_SUBJECT,
                uri = fileProvider.uriOf(exportFile),
                mimeType = EXPORT_MIME_TYPE
            )
        )
    }

    private fun writeExport(holdings: List<CoinageHolding>): File {
        val generatedAt = ZonedDateTime.now(ZoneOffset.UTC)
        // A fresh name per export: receiving apps key their preview cache on the shared file name, so a fixed
        // one makes them show the previous export instead of this one.
        val exportFile = fileProvider.getFileInInternalCacheStorage(
            "$EXPORT_FILE_PREFIX${FILE_NAME_TIMESTAMP_FORMAT.format(generatedAt)}.zip"
        )
        exportFile.delete()
        deleteStaleExports(exportFile)

        val logLines = readCoinageLogLines()

        ZipOutputStream(FileOutputStream(exportFile)).use { zipStream ->
            zipStream.putNextEntry(ZipEntry(REPORT_ENTRY_NAME))

            zipStream.bufferedWriter().run {
                appendLine("=== COINAGE DIAGNOSTICS ===")
                appendLine("Generated (UTC): ${REPORT_TIMESTAMP_FORMAT.format(generatedAt)}")
                appendLine("Log lines: ${logLines.size}")

                appendLine()
                appendLine("=== HOLDINGS (${holdings.size}) ===")
                holdings.forEach { appendLine(it.describe()) }

                appendLine()
                appendLine("=== COINAGE TRANSACTION LOGS ===")
                logLines.forEach { appendLine(it) }

                // Flushed rather than closed: closing the writer would close the zip stream with it.
                flush()
            }

            zipStream.closeEntry()
        }

        return exportFile
    }

    private fun deleteStaleExports(currentExport: File) {
        currentExport.parentFile
            ?.listFiles { file -> file.name.startsWith(EXPORT_FILE_PREFIX) && file != currentExport }
            ?.forEach { it.delete() }
    }

    // Copied before reading because the logger keeps appending to the live file, which otherwise ends the
    // export on a half-written line.
    private fun readCoinageLogLines(): List<String> {
        val coinageLogFile = fileProvider.getFileInScopedStorage(
            "${LoggerConstants.LOGS_DIR}/${LoggerConstants.COINAGE_LOGS_FILE_NAME}"
        )
        if (!coinageLogFile.exists()) return emptyList()

        val snapshotFile = fileProvider.getFileInInternalCacheStorage(LOG_SNAPSHOT_NAME)

        return try {
            coinageLogFile.copyTo(snapshotFile, overwrite = true)
            snapshotFile.readLines()
        } finally {
            snapshotFile.delete()
        }
    }

    private fun CoinageHolding.describe(): String {
        return "Holding[$id, value=${exponent.formatValue()}, exp=2^${exponent.value}, ready=$isReady, " +
            "hops=$hops, batchUnloaded=$isBatchUnloaded, fungibility=${recyclerFungibility?.percent ?: "unknown"}]"
    }

    // Matches what the Coins and Vouchers debug sheets print, so a tester can line the export up against the
    // screen they read it from. Kept private to the export rather than offered as a coinage-wide formatter.
    private fun ValueExponent.formatValue(): String {
        return "$" + String.format(Locale.US, "%.2f", tokenAmount().toDouble() / CENTS_PER_DOLLAR)
    }
}
