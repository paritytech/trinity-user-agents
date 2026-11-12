import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

private let provider = try! ProductId("provider.dot")

/// The main tab bar shows what the core asks of the app's screens: a
/// provider's screen at once while its session is on screen, and behind a
/// notice otherwise.
@MainActor
struct FundingTabBarTests {
    @Test
    func aFrameForTheSessionOnScreenIsShownAtOnce() {
        let (presenter, wireframe, interactor) = makePresenter(showing: ["intent"])

        presenter.didReceive(fundingEvent: .frameOpened(frame("intent")))

        #expect(wireframe.presentedFrames == ["intent"])
        #expect(wireframe.notices.isEmpty)
        #expect(interactor.answered.isEmpty)
    }

    @Test
    func aFrameForASessionOffScreenWaitsForTheNotice() throws {
        let (presenter, wireframe, _) = makePresenter(showing: [])

        presenter.didReceive(fundingEvent: .frameOpened(frame("intent")))
        #expect(wireframe.presentedFrames.isEmpty)

        try #require(wireframe.notices.first).onShow()
        #expect(wireframe.presentedFrames == ["intent"])
    }

    @Test
    func closingTheNoticeAnswersTheFrameDismissed() throws {
        let (presenter, wireframe, interactor) = makePresenter(showing: [])

        presenter.didReceive(fundingEvent: .frameOpened(frame("intent")))
        try #require(wireframe.notices.first).onDismiss()

        #expect(wireframe.presentedFrames.isEmpty)
        #expect(interactor.answered == [.dismissed])
    }

    @Test
    func anAskedForFrameOpensThroughTheService() throws {
        let (presenter, wireframe, interactor) = makePresenter(showing: [])
        let request = FundingFrameRequest(entryId: "intent", providerId: provider)

        presenter.didReceive(fundingEvent: .frameRequested(request))
        #expect(interactor.opened.isEmpty)

        try #require(wireframe.notices.first).onShow()
        #expect(interactor.opened == [request])
    }

    @Test
    func theServiceReachesThoseWhoWaitedForIt() async {
        let slot = FundingServiceSlot()
        let early = Task { await slot.built().first { _ in true } }
        let service = FundingService(runtime: MockFundingRuntime(), logger: MockLogger())

        slot.fill(service)

        #expect(await early.value === service)
        #expect(await slot.built().first { _ in true } === service)
    }

    private func makePresenter(
        showing: Set<String>
    ) -> (MainTabBarPresenter, FundingWireframeSpy, FundingInteractorSpy) {
        let wireframe = FundingWireframeSpy(showing: showing)
        let interactor = FundingInteractorSpy()
        let presenter = MainTabBarPresenter(
            interactor: interactor,
            wireframe: wireframe,
            chipViewModelFactory: SPATabChipViewModelFactory(flowStateProvider: SPAFlowStateProvider())
        )
        return (presenter, wireframe, interactor)
    }

    private func frame(_ entryId: String) -> FundingFrame {
        FundingFrame(
            id: UUID(),
            entryId: entryId,
            providerId: provider,
            target: .app(route: "/"),
            awaitsUserTransfer: false
        )
    }
}

private final class FundingInteractorSpy: MainTabBarInteractorInputProtocol {
    private(set) var opened: [FundingFrameRequest] = []
    private(set) var answered: [FundingFrameOutcome] = []

    func setup() {}

    func openProviderFrame(_ request: FundingFrameRequest) {
        opened.append(request)
    }

    func answer(frame _: FundingFrame.ID, with outcome: FundingFrameOutcome) {
        answered.append(outcome)
    }
}

@MainActor
private final class FundingWireframeSpy: MainTabBarWireframeProtocol {
    struct Notice {
        let onShow: () -> Void
        let onDismiss: () -> Void
    }

    private let showing: Set<String>
    private(set) var presentedFrames: [String] = []
    private(set) var notices: [Notice] = []

    init(showing: Set<String>) {
        self.showing = showing
    }

    func isShowingFunding(entryId: String) -> Bool {
        showing.contains(entryId)
    }

    func presentProviderFrame(_ frame: FundingFrame) {
        presentedFrames.append(frame.entryId)
    }

    func showProviderFrameNotice(
        for _: FundingFrameRequest,
        onShow: @escaping () -> Void,
        onDismiss: @escaping () -> Void
    ) {
        notices.append(Notice(onShow: onShow, onDismiss: onDismiss))
    }

    func showPolkadotSignIn(with _: URL, view _: MainTabBarViewProtocol?) {}
    func showPaymentsSetupNotice(isVisible _: Bool) {}
    func openProduct(productId _: ProductId) {}
    func openChat(_: ChatOpenModel) {}
    func showConnectionStatus(_: ConnectionStatusSheetViewModel, from _: MainTabBarViewProtocol?) {}
    func showSettings(from _: MainTabBarViewProtocol?) {}
    func attachFunding(service _: FundingServiceProtocol, view _: MainTabBarViewProtocol?) {}
    func presentFunding(_: FundingEntry) {}
    func closeProviderFrames(entryId _: String) {}
    func showProviderNotWorking(_: ProductId) {}
}
