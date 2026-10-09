import DesignSystem
import SwiftUI
import TrUAPIHost
import UIKit
import UIKitExt

/// "Outside pocket" over the recipient list: sending to a bank, crypto wallet
/// or card is a withdrawal, which the core's funding overlay takes from here.
extension SearchAccountViewController {
    func installOutsidePocketRow() {
        let row = UIHostingController(rootView: FundingOutsidePocketRow { [weak self] in
            Task {
                let opener = RuntimeFundingOpener(runtimeProvider: RootDependencyLocator.getDependency())
                do {
                    _ = try await opener.openFunding(direction: .out)
                } catch let error as FundingOpeningError {
                    self?.presentFundingError(error.toErrorContent())
                }
            }
        })
        row.view.backgroundColor = .clear
        row.view.frame = CGRect(x: 0, y: 0, width: view.bounds.width, height: FundingOutsidePocketRow.height)
        row.view.autoresizingMask = [.flexibleWidth]

        addChild(row)
        rootView.tableView.tableHeaderView = row.view
        row.didMove(toParent: self)
    }

    private func presentFundingError(_ content: ErrorContent) {
        let alert = UIAlertController(title: content.title, message: content.message, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: String(localized: .Common.close), style: .cancel))
        present(alert, animated: true)
    }
}

struct FundingOutsidePocketRow: View {
    static let height: CGFloat = 88

    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: DSSpacings.smallIncreased) {
                Image(systemName: "arrow.up.right")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(.fgPrimary)
                    .frame(width: 40, height: 40)
                    .background(.bgSurfaceNested, in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(.Funding.outsidePocketTitle)
                        .typography(.bodyMediumEmphasized)
                        .foregroundStyle(.fgPrimary)
                    Text(.Funding.outsidePocketSubtitle)
                        .typography(.captionMedium)
                        .foregroundStyle(.fgSecondary)
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.fgSecondary)
            }
            .padding(.horizontal, DSSpacings.medium)
            .frame(height: 64)
            .background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: DSRadii.large))
        }
        .buttonStyle(.plain)
        .padding(.horizontal, DSSpacings.medium)
        .padding(.vertical, DSSpacings.small)
    }
}
