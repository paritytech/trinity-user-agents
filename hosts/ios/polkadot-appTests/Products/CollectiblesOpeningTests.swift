import Testing
@testable import polkadot_app

/// The Collectibles page expands only once its product has loaded and the frames have settled. A load that does not
/// finish in time fails the page, and nothing after that moves the page on.
struct CollectiblesOpeningTests {
    /// A load that finished before the cover opened still settles as soon as the cover does.
    @Test
    func settlesAtTheCoverWhenTheLoadAlreadyFinished() {
        var opening = CollectiblesOpening()

        let steps = [
            opening.handle(.pageLoading(true)),
            opening.handle(.pageLoading(false)),
            opening.handle(.coverOpened)
        ]

        #expect(steps == [nil, nil, .awaitSteadyFrames])
    }

    @Test
    func settlesAtTheLoadWhenTheCoverOpenedFirst() {
        var opening = CollectiblesOpening()

        let steps = [
            opening.handle(.coverOpened),
            opening.handle(.pageLoading(true)),
            opening.handle(.pageLoading(false))
        ]

        #expect(steps == [nil, nil, .awaitSteadyFrames])
    }

    /// The web view reports a load as finished before any navigation starts, which is not a load that finished.
    @Test
    func aFinishedLoadNeedsAStartedLoad() {
        var opening = CollectiblesOpening()

        let steps = [
            opening.handle(.pageLoading(false)),
            opening.handle(.coverOpened)
        ]

        #expect(steps == [nil, nil])
        #expect(opening.stage == .opened)
    }

    @Test
    func failsWhenTheLoadTimesOutAndALaterLoadChangesNothing() {
        var opening = CollectiblesOpening()
        _ = opening.handle(.coverOpened)

        let timeout = opening.handle(.loadTimedOut)
        let laterLoad = [
            opening.handle(.pageLoading(true)),
            opening.handle(.pageLoading(false))
        ]

        #expect(timeout == .showFailure)
        #expect(laterLoad == [nil, nil])
        #expect(opening.stage == .failed)
    }

    @Test
    func aTimeoutWhileSettlingIsIgnored() {
        var opening = CollectiblesOpening()
        _ = opening.handle(.coverOpened)
        _ = opening.handle(.pageLoading(true))
        _ = opening.handle(.pageLoading(false))

        #expect(opening.handle(.loadTimedOut) == nil)
        #expect(opening.stage == .settling)
    }

    /// Frames settle into the expansion, and the page is shown once the expansion ends.
    @Test
    func expandsOnceFramesSettleAndIgnoresLaterTimeouts() {
        var opening = CollectiblesOpening()
        _ = opening.handle(.coverOpened)
        _ = opening.handle(.pageLoading(true))
        _ = opening.handle(.pageLoading(false))

        let expansion = opening.handle(.framesSettled)
        #expect(expansion == .expandPage)
        #expect(opening.stage == .expanding)

        #expect(opening.handle(.pageExpanded) == nil)
        #expect(opening.stage == .shown)
        #expect(opening.handle(.loadTimedOut) == nil)
        #expect(opening.stage == .shown)
    }

    @Test
    func framesSettleOnlyWhileSettling() {
        var opening = CollectiblesOpening()
        _ = opening.handle(.coverOpened)

        #expect(opening.handle(.framesSettled) == nil)
        #expect(opening.stage == .opened)
    }
}
