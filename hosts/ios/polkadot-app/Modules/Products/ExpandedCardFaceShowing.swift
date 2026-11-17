import TrUAPIHost

/// `shown: false` means the page covers the face; the face itself never moves.
@MainActor
protocol ExpandedCardFaceShowing: AnyObject {
    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome

    /// Turns the card so its back (`true`) or its front (`false`) is up.
    func setBackShown(_ shown: Bool) -> ExpandedCardTurnOutcome
}

/// What the page asks of the opened card.
enum ExpandedCardPageRequest: Equatable {
    case face(shown: Bool)
    case back(shown: Bool)

    /// Every answer decided in one place. A missing back is told first, so the page does not wait for the user's
    /// finger only to retry into `noBack`. `applied` means the request is to be carried out.
    func outcome(hasBack: Bool, isUserMoving: Bool, isPresented: Bool) -> ExpandedCardTurnOutcome {
        if self == .back(shown: true), !hasBack { return .noBack }
        if isUserMoving { return .userMoving }
        if !isPresented { return .notPresented }
        return .applied
    }
}

extension ExpandedCardFaceOutcome {
    /// Only a turn is told `noBack`, so a face request never meets it.
    init(_ answer: ExpandedCardTurnOutcome) {
        switch answer {
        case .applied: self = .applied
        case .userMoving: self = .userMoving
        case .notPresented,
             .noBack: self = .notPresented
        case .unsupported: self = .unsupported
        }
    }
}
