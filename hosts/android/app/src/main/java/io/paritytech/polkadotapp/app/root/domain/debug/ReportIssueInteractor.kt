package io.paritytech.polkadotapp.app.root.domain.debug

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import java.io.File
import javax.inject.Inject

private const val GITHUB_USERNAME_KEY = "issue_report_github_username"

class ReportIssueInteractor @Inject constructor(
    private val collectLogs: CollectIssueLogsUseCase,
    private val api: IssueReportApi,
    private val preferences: Preferences,
    private val dispatchers: CoroutineDispatchers,
) {
    /** GitHub username sent with the previous report, so the next one starts with it; empty when none was. */
    fun lastGitHubUsername(): String = preferences.getString(GITHUB_USERNAME_KEY).orEmpty()

    suspend fun send(description: String, gitHubUsername: GitHubUsername?, screenshot: File): Result<Unit> {
        preferences.putString(GITHUB_USERNAME_KEY, gitHubUsername?.value)
        return collectLogs().flatMap { logs ->
            try {
                api.send(IssueReport(description.trim(), logs, screenshot, gitHubUsername))
            } finally {
                withContext(NonCancellable + dispatchers.io) {
                    logs.delete()
                }
            }
        }
    }
}
