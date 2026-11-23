import DesignSystem
import SwiftUI
import UIKit

/// Hosts one funding screen's SwiftUI view, on the funding sheet's colours
/// with the navigation bar hidden. Each screen's controller draws the view
/// for every view model its presenter pushes; there is none before the
/// presenter's setup, which runs as the view loads and pushes the first.
class FundingScreenController<Content: View>: UIHostingController<Content?>, FundingFlowScreenTagged {
    let screen: FundingFlowScreen
    private let onLoad: () -> Void

    init(screen: FundingFlowScreen, onLoad: @escaping () -> Void) {
        self.screen = screen
        self.onLoad = onLoad
        super.init(rootView: nil)
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
