package io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue

import io.paritytech.polkadotapp.app.root.domain.debug.GitHubUsername
import kotlinx.coroutines.flow.StateFlow

const val ISSUE_DESCRIPTION_LIMIT = 2_000
const val ISSUE_SCREENSHOT_PATH = "issueScreenshotPath"

data class ReportIssueState(
    val description: String,
    val gitHubUsername: String,
    val screenshotPath: String,
    val isSending: Boolean,
    val isComplete: Boolean,
) {
    val isGitHubUsernameValid: Boolean
        get() = gitHubUsername.isBlank() || GitHubUsername.parse(gitHubUsername) != null
}

interface ReportIssueContract {
    val state: StateFlow<ReportIssueState>

    fun onDescriptionChanged(description: String)
    fun onGitHubUsernameChanged(gitHubUsername: String)
    fun onSendClick()
    fun onCloseClick()
}
