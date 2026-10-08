import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost
import UIKit

/// A provider's own screen, full screen over the app, for as long as the
/// provider needs it.
///
/// The user leaving through the back button is `Dismissed`. The frame closes
/// itself as `Closed` when the host sees the provider's part done: the payment
/// seen or the session over, or, for a bank transfer, the user saying they
/// have sent the funds.
final class FundingProviderFrameController: UIViewController {
    private let page: UIViewController
    private let showsSentFunds: Bool
    private var completion: ((FundingFrameOutcome) -> Void)?

    init(page: UIViewController, showsSentFunds: Bool, completion: @escaping (FundingFrameOutcome) -> Void) {
        self.page = page
        self.showsSentFunds = showsSentFunds
        self.completion = completion
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .fullScreen
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain

        let topBar = hosted(FundingFrameTopBar { [weak self] in self?.finish(.dismissed) })
        embed(page)
        embed(topBar)

        var constraints = [
            topBar.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            topBar.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            topBar.view.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            topBar.view.heightAnchor.constraint(equalToConstant: Self.topBarHeight),
            page.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            page.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            page.view.topAnchor.constraint(equalTo: topBar.view.bottomAnchor)
        ]

        if showsSentFunds {
            let bottomBar = hosted(FundingFrameSentFundsBar { [weak self] in self?.finish(.closed) })
            embed(bottomBar)
            constraints += [
                bottomBar.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
                bottomBar.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
                bottomBar.view.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
                bottomBar.view.heightAnchor.constraint(equalToConstant: Self.bottomBarHeight),
                page.view.bottomAnchor.constraint(equalTo: bottomBar.view.topAnchor)
            ]
        } else {
            constraints.append(page.view.bottomAnchor.constraint(equalTo: view.bottomAnchor))
        }

        NSLayoutConstraint.activate(constraints)
    }

    /// Answers the core once and takes the frame down.
    func finish(_ outcome: FundingFrameOutcome) {
        guard let completion else { return }
        self.completion = nil

        if presentingViewController != nil {
            dismiss(animated: true)
        }
        completion(outcome)
    }
}

private extension FundingProviderFrameController {
    static let topBarHeight: CGFloat = 56
    static let bottomBarHeight: CGFloat = 76

    func embed(_ child: UIViewController) {
        addChild(child)
        child.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(child.view)
        child.didMove(toParent: self)
    }

    func hosted(_ content: some View) -> UIViewController {
        let host = UIHostingController(rootView: content)
        host.view.backgroundColor = .clear
        return host
    }
}

private struct FundingFrameTopBar: View {
    let onBack: () -> Void

    var body: some View {
        HStack {
            FundingCircleButton(systemImage: "chevron.left", action: onBack)
            Spacer()
        }
        .padding(.horizontal, DSSpacings.medium)
        .frame(maxHeight: .infinity)
    }
}

/// A bank transfer happens outside the app, so the user says when it is done.
private struct FundingFrameSentFundsBar: View {
    let onSentFunds: () -> Void

    var body: some View {
        FundingPrimaryButton(title: String(localized: .Funding.frameSentFunds), action: onSentFunds)
            .padding(.horizontal, DSSpacings.mediumIncreased)
            .frame(maxHeight: .infinity)
    }
}
