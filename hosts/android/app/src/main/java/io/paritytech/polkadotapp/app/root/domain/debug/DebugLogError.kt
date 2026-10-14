package io.paritytech.polkadotapp.app.root.domain.debug

sealed class DebugLogError(message: String) : Throwable(message) {
    data object MissingLogs : DebugLogError("No app logs are available")

    data object ArchiveTooLarge : DebugLogError("The log archive is too large to send")
}
