import Foundation
@testable import polkadot_app

/// Every funding screen's wireframe, recording where the screens sent the
/// user and the toasts they showed, in order.
@MainActor
final class MockFundingFlowWireframe {
    enum Call: Equatable {
        case show(FundingFlowScreen)
        case showProgress
        case showDeposit
        case showHistory
        case back
        case close
        case startOver
        case toast(String)
    }

    private(set) var calls: [Call] = [] {
        didSet { changes.signal() }
    }

    let changes = StateChangeSignal()

    var toasts: [String] {
        calls.compactMap { call in
            guard case let .toast(message) = call else { return nil }
            return message
        }
    }

    func show(_ screen: FundingFlowScreen) {
        calls.append(.show(screen))
    }

    func showProgress() {
        calls.append(.showProgress)
    }

    func showDeposit() {
        calls.append(.showDeposit)
    }

    func showHistory() {
        calls.append(.showHistory)
    }

    func back() {
        calls.append(.back)
    }

    func close(then _: (() -> Void)?) {
        calls.append(.close)
    }

    func startOver() {
        calls.append(.startOver)
    }

    func showToast(_ message: String) {
        calls.append(.toast(message))
    }
}

extension MockFundingFlowWireframe: FundingAmountWireframeProtocol, FundingSummaryWireframeProtocol,
    FundingCountryWireframeProtocol, FundingNetworkWireframeProtocol, FundingTokenWireframeProtocol,
    FundingProvidersWireframeProtocol, FundingDepositWireframeProtocol, FundingProgressWireframeProtocol,
    FundingCancelConfirmWireframeProtocol {}
