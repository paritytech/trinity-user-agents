package io.paritytech.polkadotapp.app.logging

import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logging.LogFileAppender
import io.paritytech.polkadotapp.common.utils.logging.LoggerConstants
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class AppFileDebugTree @Inject constructor(
    fileProvider: FileProvider,
    coroutineDispatchers: CoroutineDispatchers
) : Timber.DebugTree() {
    private val appender = LogFileAppender(
        coroutineScope = CoroutineScope(SupervisorJob() + coroutineDispatchers.io),
        fallbackTag = "App",
        logFile = fileProvider.getFileInScopedStorage(
            "${LoggerConstants.LOGS_DIR}/${LoggerConstants.LOGS_FILE_NAME}"
        )
    )

    override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
        appender.append(priority, tag, message, t)
    }
}
