import Testing
@testable import polkadot_app

/// The Collectibles page expands only once its product has loaded. A load that does not finish in time fails the page,
/// and nothing after that moves the page on.
struct CollectiblesOpeningTests {
    /// A load that finished before the cover opened still expands the page as soon as the cover does.
    @Test
    func expandsAtTheCoverWhenTheLoadAlreadyFinished() {
        var opening = CollectiblesOpening()

        let steps = [
            opening.handle(.pageLoading(true)),
            opening.handle(.pageLoading(false)),
            opening.handle(.coverOpened)
        ]

        #expect(steps == [nil, nil, .expandPage])
    }

    @Test
    func expandsAtTheLoadWhenTheCoverOpenedFirst() {
        var opening = CollectiblesOpening()

        let steps = [
            opening.handle(.coverOpened),
            opening.handle(.pageLoading(true)),
            opening.handle(.pageLoading(false))
        ]

        #expect(steps == [nil, nil, .expandPage])
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
    func aTimeoutWhileExpandingIsIgnored() {
        var opening = CollectiblesOpening()
        _ = opening.handle(.coverOpened)
        _ = opening.handle(.pageLoading(true))
        _ = opening.handle(.pageLoading(false))

        #expect(opening.handle(.loadTimedOut) == nil)
        #expect(opening.stage == .expanding)
    }

    /// The page is shown once the expansion ends.
    @Test
    func isShownOnceTheExpansionEndsAndIgnoresLaterTimeouts() {
        var opening = CollectiblesOpening()
        _ = opening.handle(.coverOpened)
        _ = opening.handle(.pageLoading(true))
        let expansion = opening.handle(.pageLoading(false))

        #expect(expansion == .expandPage)
        #expect(opening.stage == .expanding)

        #expect(opening.handle(.pageExpanded) == nil)
        #expect(opening.stage == .shown)
        #expect(opening.handle(.loadTimedOut) == nil)
        #expect(opening.stage == .shown)
    }
}
