import Foundation
import Products
import UIKit
import UIKitExt
@testable import polkadot_app

/// A product web view that loads nothing, for the screens and coordinators that
/// only care about where the view was put and what it was told to show.
///
/// The pages are recorded because a coordinator's whole job is deciding which
/// page a tab lands on, and a page navigated to twice is the defect.
@MainActor
final class StubSPAView: SPAViewProtocol {
    private(set) var navigatedPages: [ProductPage] = []
    let controller = UIViewController()
    let pageScrollView = UIScrollView()
    var isSetup: Bool { true }

    func navigate(to _: URL) {}
    func navigate(to page: ProductPage) { navigatedPages.append(page) }
    func updateTitle(_: String) {}
    func reload() {}
    func showLoading() {}
    func hideLoading() {}
    func showLoadFailure(_: ErrorContent) {}
    func updateLoadProgress(_: DotNsLoadProgress) {}
}
