package io.paritytech.polkadotapp.app.logging

import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logging.LogFileAppender
import io.paritytech.polkadotapp.common.utils.logging.LoggerConstants
import io.paritytech.polkadotapp.feature_coinage_impl.domain.COINAGE_LOG_TAG
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DURABILITY_LOG_TAG
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

// Extends Timber.Tree rather than DebugTree: this tree is planted in every build type, and DebugTree
// resolves a class-name tag before isLoggable runs, which would walk a stack trace on every untagged log
// line in the app only to reject it here.
@Singleton
class CoinageFileTree @Inject constructor(
    fileProvider: FileProvider,
    coroutineDispatchers: CoroutineDispatchers
) : Timber.Tree() {
    private companion object {
        // Durability lines carry the engine's tag: the verdicts and the reads behind them live there, and a
        // coinage log without them shows a status changing for no reason.
        val ROUTED_TAGS = setOf(COINAGE_LOG_TAG, DURABILITY_LOG_TAG)
    }

    private val appender = LogFileAppender(
        coroutineScope = CoroutineScope(SupervisorJob() + coroutineDispatchers.io),
        fallbackTag = COINAGE_LOG_TAG,
        logFile = fileProvider.getFileInScopedStorage(
            "${LoggerConstants.LOGS_DIR}/${LoggerConstants.COINAGE_LOGS_FILE_NAME}"
        )
    )

    override fun isLoggable(tag: String?, priority: Int): Boolean = tag in ROUTED_TAGS

    override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
        appender.append(priority, tag, message, t)
    }
}
