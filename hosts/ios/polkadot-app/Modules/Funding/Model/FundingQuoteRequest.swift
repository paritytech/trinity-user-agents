import TrUAPIHost

/// Which of the core's quote lists the overlay shows: only the latest ask's,
/// once the core returned that ask's request id. A list can arrive before the
/// id does; the newest such list is kept until the id comes back.
struct FundingQuoteRequest {
    /// Counts the asks and the drops, so the id of an ask replaced while it
    /// was on its way is not taken.
    private var askNumber = 0
    /// The id whose lists are shown; nil while no ask is current or its id
    /// is on its way.
    private var requestId: UInt64?
    /// Whether an ask's id is on its way.
    private(set) var awaitsId = false
    private var early: FundingQuoteList?

    /// Starts a new ask, and returns its number for ``take(_:ofAsk:)``.
    mutating func ask() -> Int {
        drop()
        awaitsId = true
        return askNumber
    }

    /// Shows no list of any ask made so far, including one whose id is still
    /// on its way.
    mutating func drop() {
        askNumber += 1
        requestId = nil
        awaitsId = false
        early = nil
    }

    /// Whether ask `number` is the latest, with no drop since.
    func isLatest(_ number: Int) -> Bool {
        number == askNumber
    }

    /// Takes the core's id for ask `number`, unless a later ask or a drop
    /// replaced it, and returns the list that arrived before the id and
    /// answers that ask.
    mutating func take(_ id: UInt64, ofAsk number: Int) -> FundingQuoteList? {
        guard number == askNumber else { return nil }

        requestId = id
        awaitsId = false
        defer { early = nil }
        return early?.requestId == id ? early : nil
    }

    /// Whether `list` answers the current ask. While the ask's id is on its
    /// way, keeps the newest list instead, which may answer it.
    mutating func shows(_ list: FundingQuoteList) -> Bool {
        guard let requestId else {
            if awaitsId, list.requestId >= early?.requestId ?? 0 {
                early = list
            }
            return false
        }
        return list.requestId == requestId
    }
}
