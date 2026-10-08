import DesignSystem
import SwiftUI
import TrUAPIHost
import UIKit

/// "Outside pocket" over the recipient list: sending to a bank, crypto wallet
/// or card is a withdrawal, which the core's funding overlay takes from here.
extension SearchAccountViewController {
    func installOutsidePocketRow() {
        let row = UIHostingController(rootView: FundingOutsidePocketRow {
            Task {
                let opener = RuntimeFundingOpener(runtimeProvider: RootDependencyLocator.getDependency())
                _ = try? await opener.openFunding(direction: .out)
            }
        })
        row.view.backgroundColor = .clear
        row.view.frame = CGRect(x: 0, y: 0, width: view.bounds.width, height: FundingOutsidePocketRow.height)
        row.view.autoresizingMask = [.flexibleWidth]

        addChild(row)
        rootView.tableView.tableHeaderView = row.view
        row.didMove(toParent: self)
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
