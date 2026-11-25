import Foundation
import TrUAPIHost

/// The quotes of one session: only the latest ask's list is shown.
struct FundingSessionQuotes {
    var request = FundingQuoteRequest()
    var shown: FundingQuoteList?
    var isAwaiting = false
    /// The session is handed to the latest ask's best quote once every
    /// provider answered.
    var startsWhenQuoted = false
    /// The core's id for the latest ask, once it returned it.
    var requestId: UInt64?
    /// The ask the core was last told to start the session on.
    var startedOn: UInt64?

    mutating func ask() -> (number: Int, shownId: UInt64?) {
        isAwaiting = true
        requestId = nil
        return (request.ask(), shown?.requestId)
    }

    mutating func drop() {
        request.drop()
        shown = nil
        isAwaiting = false
        requestId = nil
    }
}
