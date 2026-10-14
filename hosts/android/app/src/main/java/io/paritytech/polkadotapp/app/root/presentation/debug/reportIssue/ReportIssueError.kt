package io.paritytech.polkadotapp.app.root.presentation.debug.reportIssue

import io.paritytech.polkadotapp.app.root.domain.debug.DebugLogError
import io.paritytech.polkadotapp.app.root.domain.debug.IssueReportSubmissionError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.R as RCommon

class ReportIssueError(cause: Throwable) : Exception(cause),
    PresentationError by StringResPresentationError(
        when (cause) {
            IssueReportSubmissionError.NotConfigured -> RCommon.string.debug_report_unavailable
            IssueReportSubmissionError.AttachmentsTooLarge, DebugLogError.ArchiveTooLarge -> RCommon.string.debug_report_too_large
            else -> RCommon.string.debug_report_failed
        }
    )
