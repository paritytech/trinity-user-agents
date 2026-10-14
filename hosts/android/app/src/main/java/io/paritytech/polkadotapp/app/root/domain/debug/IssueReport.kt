package io.paritytech.polkadotapp.app.root.domain.debug

import java.io.File

data class IssueReport(
    val description: String,
    val logs: File,
    val screenshot: File,
)

fun interface IssueReportApi {
    /** Returns success only after the proxy confirms issue creation. */
    suspend fun send(report: IssueReport): Result<Unit>
}
