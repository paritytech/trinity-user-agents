import DesignSystem
import Observation
import SwiftUI
import UIKit
import UIKitExt

/// What every funding screen's presenter draws through: the flow's state,
/// and the screen's text from its view-model factory.
@MainActor
protocol FundingScreenViewProtocol<ViewModel>: ControllerBackedProtocol {
    associatedtype ViewModel

    func didReceive(state: FundingFlowState, viewModel: ViewModel)
}

/// The state one funding screen draws, read by its view as if its own
/// properties, and its text.
@MainActor
@Observable
@dynamicMemberLookup
final class FundingScreenModel<ViewModel> {
    var state: FundingFlowState
    var viewModel: ViewModel

    init(state: FundingFlowState, viewModel: ViewModel) {
        self.state = state
        self.viewModel = viewModel
    }

    subscript<Value>(dynamicMember keyPath: KeyPath<FundingFlowState, Value>) -> Value {
        state[keyPath: keyPath]
    }
}

/// Hosts one funding screen's SwiftUI view, on the funding sheet's colours
/// with the navigation bar hidden, and hands the presenter's state to it.
final class FundingScreenController<Content: View, ViewModel>: UIHostingController<Content>,
    FundingScreenViewProtocol, FundingFlowScreenTagged {
    let screen: FundingFlowScreen
    let model: FundingScreenModel<ViewModel>
    private let onLoad: () -> Void

    init(
        screen: FundingFlowScreen,
        model: FundingScreenModel<ViewModel>,
        content: Content,
        onLoad: @escaping () -> Void
    ) {
        self.screen = screen
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
        view.backgroundColor = .bgSurfaceContainer
        onLoad()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        navigationController?.setNavigationBarHidden(true, animated: false)
    }

    func didReceive(state: FundingFlowState, viewModel: ViewModel) {
        model.state = state
        model.viewModel = viewModel
    }
}

extension View {
    /// Lays a screen out on the funding sheet.
    func fundingScreen() -> some View {
        padding(DSSpacings.mediumIncreased)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(Color.bgSurfaceContainer.ignoresSafeArea())
            .toolbar(.hidden, for: .navigationBar)
    }
}
