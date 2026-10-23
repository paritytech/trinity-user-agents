import Foundation
import TrUAPIHost

/// Puts back into the host's history every session the core recorded that
/// the store lacks, such as after the store was lost. A row the store has
/// wins. Runs once per launch, and again only if reading the store failed.
@MainActor
final class FundingHistoryRebuilder {
    private let store: FundingHistoryStoring
    private var done = false

    init(store: FundingHistoryStoring) {
        self.store = store
    }

    func rebuildOnce(reading history: () -> [FundingSessionSummary]) async {
        guard !done else { return }

        let storedIntents: Set<String>
        do {
            storedIntents = try await Set(store.records().map(\.intent))
        } catch {
            Logger.shared.error("[funding] reading the history to rebuild it failed: \(error)")
            return
        }
        done = true

        let missing = history()
            .filter { !storedIntents.contains($0.intent) }
            .compactMap(FundingRecord.init(summary:))
        for record in missing {
            do {
                try await store.save(record)
            } catch {
                Logger.shared.error("[funding] \(record.intent): rebuilding its history row failed: \(error)")
            }
        }
    }
}
