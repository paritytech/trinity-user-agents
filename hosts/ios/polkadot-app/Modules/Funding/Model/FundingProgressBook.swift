import Foundation
import os
import TrUAPIHost

/// The progress the core last handed over for each funding session. The core
/// sends it with every change, and again for every session when funding
/// resumes, so the screens read it here rather than from the core.
final class FundingProgressBook: Sendable {
    static let shared = FundingProgressBook()

    private let entries = OSAllocatedUnfairLock(initialState: [String: FundingProgress]())

    func progress(intent: String) -> FundingProgress? {
        entries.withLock { $0[intent] }
    }

    func record(intent: String, progress: FundingProgress?) {
        entries.withLock { $0[intent] = progress }
    }
}
