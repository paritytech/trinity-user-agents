/// The opening of the Collectibles book. The cover swings open while the product loads, and the page expands once
/// the load has finished and the frames have settled. A load that does not finish in time fails the page instead.
struct CollectiblesOpening {
    enum Stage: Equatable {
        case closed
        case opened
        case settling
        case expanding
        case shown
        case failed
    }

    enum Event: Equatable {
        case coverOpened
        case pageLoading(Bool)
        case loadTimedOut
        case framesSettled
        case pageExpanded
    }

    enum Step: Equatable {
        case awaitSteadyFrames
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
            return awaitStepIfLoaded()
        case let (_, .pageLoading(isLoading)):
            if isLoading {
                loadStarted = true
            } else if loadStarted {
                loadFinished = true
            }
            return awaitStepIfLoaded()
        case (.opened, .loadTimedOut):
            stage = .failed
            return .showFailure
        case (.settling, .framesSettled):
            stage = .expanding
            return .expandPage
        case (.expanding, .pageExpanded):
            stage = .shown
            return nil
        default:
            return nil
        }
    }

    private mutating func awaitStepIfLoaded() -> Step? {
        guard stage == .opened, loadFinished else { return nil }

        stage = .settling
        return .awaitSteadyFrames
    }
}
