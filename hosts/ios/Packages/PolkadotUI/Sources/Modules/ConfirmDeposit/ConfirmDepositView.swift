import SwiftUI
import DesignSystem

public struct ConfirmDepositView: View {
    public var amount: String
    public var isLoading: Bool = false
    public var onConfirmTapped: () -> Void = {}

    public init(
        amount: String,
        isLoading: Bool = false,
        onConfirmTapped: @escaping () -> Void = {}
    ) {
        self.amount = amount
        self.isLoading = isLoading
        self.onConfirmTapped = onConfirmTapped
    }

    public var body: some View {
        VStack(spacing: 0) {
            Spacer()

            BottomSheetBaseView {
                VStack(alignment: .center, spacing: 0) {
                    Text(String(localized: .confirmDepositTitle))
                        .typography(.headlineSmall)
                        .foregroundColor(Color(.textAndIconsPrimaryDark))
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 16)

                    Text(String(localized: .confirmDepositDescription))
                        .typography(.paragraphLarge)
                        .foregroundColor(Color(.textAndIconsSecondary))
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 12)

                    Text(amount)
                        .font(.system(size: 72, weight: .semibold))
                        .foregroundColor(Color(.textAndIconsPrimaryDark))
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 80)
                        .padding(.bottom, 80)

                    Button(action: onConfirmTapped) {
                        Group {
                            if isLoading {
                                ProgressView()
                                    .progressViewStyle(
                                        CircularProgressViewStyle(tint: Color(.textAndIconsPrimaryLight))
                                    )
                            } else {
                                Text(String(localized: .confirmDepositButton))
                                    .typography(.titleMedium)
                            }
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.mainWhite)
                    .disabled(isLoading)
                }
            }
        }
    }
}

#Preview {
    ConfirmDepositView(amount: "$5", isLoading: false)
}
