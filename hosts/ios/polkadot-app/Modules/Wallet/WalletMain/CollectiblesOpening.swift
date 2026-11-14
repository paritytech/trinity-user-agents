/// The opening of the Collectibles book. The cover swings open while the product loads, and the page expands once
/// the load has finished. A load that does not finish in time fails the page instead.
struct CollectiblesOpening {
    enum Stage: Equatable {
        case closed
        case opened
        case expanding
        case shown
        case failed
    }

    enum Event: Equatable {
        case coverOpened
        case pageLoading(Bool)
        case loadTimedOut
        case pageExpanded
    }

    enum Step: Equatable {
        case expandPage
        case showFailure
    }

    private(set) var stage: Stage = .closed
    private var loadStarted = false
    private var loadFinished = false

    /// Moves the opening on for `event`, and returns the work the screen must start for it.
    mutating func handle(_ event: Event) -> Step? {
        switch (stage, event) {
        case (.closed, .coverOpened):
            stage = .opened
            return expandStepIfLoaded()
        case let (_, .pageLoading(isLoading)):
            if isLoading {
                loadStarted = true
            } else if loadStarted {
                loadFinished = true
            }
            return expandStepIfLoaded()
        case (.opened, .loadTimedOut):
            stage = .failed
            return .showFailure
        case (.expanding, .pageExpanded):
            stage = .shown
            return nil
        default:
            return nil
        }
    }

    private mutating func expandStepIfLoaded() -> Step? {
        guard stage == .opened, loadFinished else { return nil }

        stage = .expanding
        return .expandPage
    }
}
