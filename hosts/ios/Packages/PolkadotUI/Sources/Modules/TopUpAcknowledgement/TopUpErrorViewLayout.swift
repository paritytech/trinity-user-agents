import SwiftUI
import DesignSystem

public struct TopUpErrorViewLayout: View {
    public var title: String
    public var message: String
    public var closeButtonTitle: String
    public var onCloseTapped: () -> Void = {}

    public init(
        title: String = "",
        message: String = "",
        closeButtonTitle: String = "",
        onCloseTapped: @escaping () -> Void = {}
    ) {
        self.title = title
        self.message = message
        self.closeButtonTitle = closeButtonTitle
        self.onCloseTapped = onCloseTapped
    }

    public var body: some View {
        VStack(spacing: 0) {
            Spacer()

            BottomSheetBaseView {
                VStack(alignment: .center, spacing: 0) {
                    Text(title)
                        .typography(.headlineSmall)
                        .foregroundColor(.fgPrimary)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 16)

                    Text(message)
                        .typography(.bodyMedium)
                        .foregroundColor(.fgError)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 24)
                        .padding(.bottom, 56)

                    DSButton(
                        closeButtonTitle,
                        style: .secondary,
                        expands: true,
                        action: onCloseTapped
                    )
                }
            }
        }
    }
}

#Preview {
    TopUpErrorViewLayout(
        title: "Failed to Accept Funds",
        message: "Coins did not appear on-chain in time.",
        closeButtonTitle: "Close"
    )
}
