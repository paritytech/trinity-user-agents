import Foundation
import Observation
import PolkadotUI

@Observable
final class GameRoomPillViewModel {
    enum Content: Hashable {
        struct LiveProgress: Hashable {
            let currentRound: Int
            let totalRounds: Int

            init(
                currentRound: Int,
                totalRounds: Int
            ) {
                self.currentRound = currentRound
                self.totalRounds = totalRounds
            }
        }

        case waiting(gameDate: Date)
        case live(LiveProgress)
        case finished
    }

    let content: Content
    var onTap: () -> Void = {}

    private let countdownFormatter: CountdownDateFormatting

    init(
        content: Content,
        countdownFormatter: CountdownDateFormatting = CountdownDateFormatter()
    ) {
        self.content = content
        self.countdownFormatter = countdownFormatter
    }

    convenience init(
        gameDate: Date,
        countdownFormatter: CountdownDateFormatting = CountdownDateFormatter()
    ) {
        self.init(
            content: .waiting(gameDate: gameDate),
            countdownFormatter: countdownFormatter
        )
    }

    convenience init(
        liveProgress: Content.LiveProgress,
        countdownFormatter: CountdownDateFormatting = CountdownDateFormatter()
    ) {
        self.init(
            content: .live(liveProgress),
            countdownFormatter: countdownFormatter
        )
    }

    var title: String {
        switch content {
        case .waiting:
            String(localized: .Game.gameRoomPillWaitingTitle)
        case .live:
            String(localized: .Game.gameRoomPillLiveTitle)
        case .finished:
            String(localized: .Game.gameRoomPillFinishedTitle)
        }
    }

    func valueString(now: Date = .now) -> String {
        switch content {
        case .waiting:
            countdownString(now: now)
        case let .live(progress):
            liveRoundString(progress: progress)
        case .finished:
            String(localized: .Game.gameRoomPillFinishedValue)
        }
    }

    var staticValueString: String {
        switch content {
        case .waiting:
            ""
        case let .live(progress):
            liveRoundString(progress: progress)
        case .finished:
            String(localized: .Game.gameRoomPillFinishedValue)
        }
    }

    func countdownString(now: Date = .now) -> String {
        guard case let .waiting(gameDate) = content else {
            return ""
        }

        guard gameDate > now else {
            return countdownFormatter.formatWithSinglePart(to: now)
        }

        return countdownFormatter.formatWithMultipleParts(to: gameDate)
    }
}

private extension GameRoomPillViewModel {
    func liveRoundString(
        progress: Content.LiveProgress
    ) -> String {
        String(
            localized: .Game.gameRoomPillLiveRound(
                currentRound: progress.currentRound,
                totalRounds: progress.totalRounds
            )
        )
    }
}
