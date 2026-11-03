import PolkadotUI
import UIKit
import UIKitExt
@testable import polkadot_app

final class MockSearchContactView: UIViewController, SearchContactViewProtocol {
    private(set) var viewModel: SearchContactResultsView.ViewModel?

    func didReceive(viewModel: SearchContactResultsView.ViewModel) {
        self.viewModel = viewModel
    }

    func didReceive(status _: SearchContactResultsView.StatusViewModel) {}
}
