import Foundation

enum GameRoomPillState: Hashable {
    case starting(gameDate: Date)
    case live(
        currentRound: Int,
        totalRounds: Int
    )
    case finished

    static func resolve(
        gameInfo: GameInfo?,
        gameTimelineState: GameStateMachine.State?,
        now: Date = .now
    ) -> GameRoomPillState? {
        if let finishedState = resolveFinishedState(
            gameInfo: gameInfo,
            gameTimelineState: gameTimelineState
        ) {
            return finishedState
        }

        if let liveState = resolveLiveState(
            gameTimelineState: gameTimelineState,
            gameInfo: gameInfo
        ) {
            return liveState
        }

        return resolveStartingState(gameInfo: gameInfo, now: now)
    }
}

private extension GameRoomPillState {
    static func resolveFinishedState(
        gameInfo: GameInfo?,
        gameTimelineState: GameStateMachine.State?
    ) -> GameRoomPillState? {
        guard
            let gameInfo,
            case let .finished(finishedInfo) = gameTimelineState,
            finishedInfo.gameIndex == gameInfo.index,
            gameInfo.isReportSent != true,
            gameInfo.isRegistered
        else {
            return nil
        }

        return .finished
    }

    static func resolveLiveState(
        gameTimelineState: GameStateMachine.State?,
        gameInfo: GameInfo?
    ) -> GameRoomPillState? {
        switch gameTimelineState {
        case let .round(_, roundsInfo):
            guard
                let gameInfo,
                gameInfo.isRegistered,
                roundsInfo.gameIndex == gameInfo.index,
                roundsInfo.subroundsCount > 0
            else {
                return nil
            }

            return .live(
                currentRound: min(roundsInfo.subroundIndex + 1, roundsInfo.subroundsCount),
                totalRounds: roundsInfo.subroundsCount
            )

        case .preparing,
             .finished,
             nil:
            return nil
        }
    }

    static func resolveStartingState(
        gameInfo: GameInfo?,
        now: Date
    ) -> GameRoomPillState? {
        guard
            let gameInfo,
            let gameDate = gameInfo.gameDate,
            gameInfo.isGameRoomAvailable(
                now: now,
                availabilityInterval: Constants.startingPillLeadTime
            )
        else {
            return nil
        }

        return .starting(gameDate: gameDate)
    }
}

extension GameRoomPillState {
    enum Constants {
        static let startingPillLeadTime: TimeInterval = 5 * TimeInterval.secondsInMinute
    }
}
