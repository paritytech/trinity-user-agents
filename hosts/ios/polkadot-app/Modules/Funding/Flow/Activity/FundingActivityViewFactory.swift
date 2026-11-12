import SwiftUI
import UIKit

@MainActor
enum FundingActivityViewFactory {
    static func createView(
        mode: FundingActivityMode,
        cash: FundingCash,
        service: FundingServiceProtocol,
        router: FundingFlowRouting
    ) -> UIViewController {
        let interactor = FundingActivityInteractor(service: service)
        let wireframe = FundingActivityWireframe(router: router)
        let presenter = FundingActivityPresenter(mode: mode, interactor: interactor, wireframe: wireframe)
        interactor.presenter = presenter

        let model = FundingActivityViewModel()
        let content =
            switch mode {
            case .history:
                AnyView(FundingHistoryView(model: model, presenter: presenter, cash: cash).fundingScreen())
            case let .pending(direction):
                AnyView(
                    FundingPendingView(direction: direction, model: model, presenter: presenter, cash: cash)
                        .fundingScreen()
                )
            }
        let view = FundingActivitySheetController(model: model, content: content, onLoad: presenter.setup)
        presenter.view = view
        wireframe.sheet = view
        return view
    }
}

/// A session list, on the same sheet as a funding flow.
final class FundingActivitySheetController: FundingSheetHostingController<AnyView>, FundingActivityViewProtocol {
    private let model: FundingActivityViewModel
    private let onLoad: () -> Void

    init(model: FundingActivityViewModel, content: AnyView, onLoad: @escaping () -> Void) {
        self.model = model
        self.onLoad = onLoad
        super.init(rootView: content)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        onLoad()
    }

    func didReceive(state: FundingActivityState) {
        model.state = state
    }
}
