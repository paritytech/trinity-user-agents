import SwiftUI
import DesignSystem

@MainActor
public protocol TopUpMismatchViewModelProtocol: AnyObject {
    var title: String { get }
    var claimedAmount: String { get }
    var originalAmount: String { get }
    var tokenSymbol: String { get }
    var subtitle: String { get }
    var closeButtonTitle: String { get }
    var onCloseTapped: () -> Void { get set }
}

@Observable
public final class TopUpMismatchViewModel: TopUpMismatchViewModelProtocol {
    public var title: String
    public var claimedAmount: String
    public var originalAmount: String
    public var tokenSymbol: String
    public var subtitle: String
    public var closeButtonTitle: String
    public var onCloseTapped: () -> Void

    public init(
        title: String = "",
        claimedAmount: String = "",
        originalAmount: String = "",
        tokenSymbol: String = "",
        subtitle: String = "",
        closeButtonTitle: String = "",
        onCloseTapped: @escaping () -> Void = {}
    ) {
        self.title = title
        self.claimedAmount = claimedAmount
        self.originalAmount = originalAmount
        self.tokenSymbol = tokenSymbol
        self.subtitle = subtitle
        self.closeButtonTitle = closeButtonTitle
        self.onCloseTapped = onCloseTapped
    }
}

public struct TopUpMismatchViewLayout: View {
    @State public private(set) var viewModel: any TopUpMismatchViewModelProtocol

    public init(viewModel: any TopUpMismatchViewModelProtocol) {
        _viewModel = State(initialValue: viewModel)
    }

    public var body: some View {
        VStack(spacing: 0) {
            Spacer()

            BottomSheetBaseView {
                VStack(alignment: .center, spacing: 0) {
                    Text(viewModel.title)
                        .typography(.headlineSmall)
                        .foregroundColor(.fgPrimary)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 16)

                    VStack(spacing: 0) {
                        Text(viewModel.originalAmount)
                            .typography(.bodyMedium)
                            .foregroundColor(.fgSecondary)
                            .strikethrough()
                            .multilineTextAlignment(.center)

                        Text(viewModel.claimedAmount)
                            .typography(.headlineLarge)
                            .foregroundColor(.fgPrimary)
                            .multilineTextAlignment(.center)

                        Text(viewModel.tokenSymbol)
                            .typography(.bodyMedium)
                            .foregroundColor(.fgSecondary)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(16)
                    .background(Color.bgSurfaceNested, in: RoundedRectangle(cornerRadius: 12))
                    .padding(.top, 32)

                    Text(viewModel.subtitle)
                        .typography(.bodyMedium)
                        .foregroundColor(.fgWarning)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 16)
                        .padding(.bottom, 32)

                    DSButton(
                        viewModel.closeButtonTitle,
                        style: .secondary,
                        expands: true,
                        action: viewModel.onCloseTapped
                    )
                }
            }
        }
    }
}

#Preview {
    TopUpMismatchViewLayout(
        viewModel: TopUpMismatchViewModel(
            title: "Funds Accepted",
            claimedAmount: "3",
            originalAmount: "5",
            tokenSymbol: "CASH",
            subtitle: "Amount differs",
            closeButtonTitle: "Close"
        )
    )
}
