package io.paritytech.polkadotapp.app.root.domain.debug

sealed class IssueReportSubmissionError : Exception() {
    data object NotConfigured : IssueReportSubmissionError()
    data object AttachmentsTooLarge : IssueReportSubmissionError()
    data class HttpFailure(val statusCode: Int) : IssueReportSubmissionError()
}
