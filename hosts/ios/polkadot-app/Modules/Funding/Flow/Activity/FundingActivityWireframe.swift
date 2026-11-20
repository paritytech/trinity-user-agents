import Foundation
import SwiftUI
import TrUAPIHost
import UIKit

@MainActor
final class FundingActivityWireframe: FundingActivityWireframeProtocol {
    weak var sheet: FundingSheetHostingController<AnyView>?
    private weak var router: FundingFlowRouting?

    init(router: FundingFlowRouting) {
        self.router = router
    }

    func open(entryId: String) {
        router?.reopen(entryId: entryId)
    }

    func showHistory() {
        let router = router
        dismiss { router?.showHistory() }
    }

    func startNew(direction: FundingDirection) {
        let router = router
        dismiss { router?.startOver(direction: direction) }
    }

    func back() {
        dismiss {}
    }
}

private extension FundingActivityWireframe {
    func dismiss(then completion: @escaping () -> Void) {
        guard let sheet else {
            completion()
            return
        }
        sheet.dismissIfPresented(completion: completion)
    }
}
