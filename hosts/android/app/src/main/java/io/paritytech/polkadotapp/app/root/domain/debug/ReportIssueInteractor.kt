package io.paritytech.polkadotapp.app.root.domain.debug

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import java.io.File
import javax.inject.Inject

class ReportIssueInteractor @Inject constructor(
    private val collectLogs: CollectIssueLogsUseCase,
    private val api: IssueReportApi,
    private val dispatchers: CoroutineDispatchers,
) {
    suspend fun send(description: String, screenshot: File): Result<Unit> = collectLogs().flatMap { logs ->
        try {
            api.send(IssueReport(description.trim(), logs, screenshot))
        } finally {
            withContext(NonCancellable + dispatchers.io) {
                logs.delete()
            }
        }
    }
}
