import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

private let provider = try! ProductId("provider.dot")

/// The main tab bar shows what the core asks of the app's screens: a
/// provider's screen opens at once while its session is on screen, and after
/// a tap on a notice otherwise.
@MainActor
struct FundingTabBarTests {
    @Test
    func anAskForTheSessionOnScreenOpensAtOnce() {
        let (presenter, wireframe, interactor) = makePresenter(showing: ["intent"])

        presenter.didReceive(fundingEvent: .frameRequested(request("intent")))

        #expect(interactor.opened == [request("intent")])
        #expect(wireframe.notices.isEmpty)
    }

    @Test
    func anAskForASessionOffScreenOpensFromTheNotice() throws {
        let (presenter, wireframe, interactor) = makePresenter(showing: [])

        presenter.didReceive(fundingEvent: .frameRequested(request("intent")))
        #expect(interactor.opened.isEmpty)

        let show = try #require(wireframe.notices.first)
        show()
        #expect(interactor.opened == [request("intent")])
    }

    @Test
    func anOpenedScreenIsShown() {
        let (presenter, wireframe, _) = makePresenter(showing: [])

        presenter.didReceive(fundingEvent: .presentFrame(frame("intent")))

        #expect(wireframe.presentedFrames == ["intent"])
        #expect(wireframe.notices.isEmpty)
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

    private func request(_ entryId: String) -> FundingFrameRequest {
        FundingFrameRequest(entryId: entryId, providerId: provider)
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

    func setup() {}

    func openProviderFrame(_ request: FundingFrameRequest) {
        opened.append(request)
    }
}

@MainActor
private final class FundingWireframeSpy: MainTabBarWireframeProtocol {
    private let showing: Set<String>
    private(set) var presentedFrames: [String] = []
    /// What each notice's Show does.
    private(set) var notices: [() -> Void] = []

    init(showing: Set<String>) {
        self.showing = showing
    }

    func isShowingFunding(entryId: String) -> Bool {
        showing.contains(entryId)
    }

    func presentProviderFrame(_ frame: FundingFrame) {
        presentedFrames.append(frame.entryId)
    }

    func showProviderFrameNotice(for _: FundingFrameRequest, onShow: @escaping () -> Void) {
        notices.append(onShow)
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
    func showBackupSettings(from _: MainTabBarViewProtocol?) {}
}
