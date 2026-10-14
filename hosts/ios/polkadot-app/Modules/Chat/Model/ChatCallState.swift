import Foundation

extension Chat {
    enum CallState: Equatable {
        case calling
        case active
        case finished(durationMs: UInt64)
        case cancelled(ringDurationMs: UInt64)
        case missed
    }
}

extension Chat.LocalMessage {
    var callOffer: Chat.RelatedLocalMessage? {
        callGroup.first { member in
            if case .call(.offer) = member.content { return true }
            return false
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    func resolveCallState() -> Chat.CallState? {
        guard let offer = callOffer else { return nil }

        var earliestAnswer: Chat.RelatedLocalMessage?
        var earliestClosed: Chat.RelatedLocalMessage?

        for member in callGroup {
            guard case let .call(payload) = member.content else { continue }
            switch payload {
            case .answer:
                if let existing = earliestAnswer, existing.timestamp <= member.timestamp { continue }
                earliestAnswer = member
            case .closed:
                if let existing = earliestClosed, existing.timestamp <= member.timestamp { continue }
                earliestClosed = member
            case .offer,
                 .candidates:
                continue
            }
        }

        guard let closed = earliestClosed else {
            return earliestAnswer != nil ? .active : .calling
        }

        if let answer = earliestAnswer {
            return .finished(durationMs: closed.timestamp.saturatingSubtract(answer.timestamp))
        }

        switch offer.status {
        case .outgoing:
            return .cancelled(ringDurationMs: closed.timestamp.saturatingSubtract(offer.timestamp))
        case .incoming:
            return .missed
        }
    }

    private var callGroup: [Chat.RelatedLocalMessage] {
        [asRelated] + relatedMessages
    }
}

private extension Chat.Timestamp {
    func saturatingSubtract(_ other: Chat.Timestamp) -> Chat.Timestamp {
        self >= other ? self - other : 0
    }
}
