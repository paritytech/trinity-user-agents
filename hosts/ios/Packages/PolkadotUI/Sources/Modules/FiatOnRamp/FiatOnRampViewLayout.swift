import Foundation
import SwiftUI

public struct FiatOnRampViewLayout: View {
    @Bindable var viewModel: FiatOnRampViewModel

    public init(viewModel: FiatOnRampViewModel) {
        self.viewModel = viewModel
    }

    private let amountFormatter: NumberFormatter = {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.allowsFloats = false
        formatter.maximumFractionDigits = 0
        formatter.minimumFractionDigits = 0
        formatter.usesGroupingSeparator = false
        return formatter
    }()

    private var continueTextColor: Color {
        viewModel.isContinueEnabled ? .black : Color(.textAndIconsDisabled)
    }

    private var continueBackground: Color {
        viewModel.isContinueEnabled ? .white : Color(.fill6)
    }

    private var quickAmountsTopPadding: CGFloat {
        viewModel.amountError == nil ? 40 : 20
    }

    public var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 2) {
                Text("$")
                    .textStyle(.title56SemiBold())
                    .foregroundStyle(.white)

                TextField(
                    "",
                    value: $viewModel.bindableAmount,
                    formatter: amountFormatter,
                    prompt: Text("0").foregroundStyle(.white.opacity(0.25))
                )
                .font(Font(UIFont.title56SemiBold()))
                .textCase(.uppercase)
                .foregroundStyle(.white)
                .tint(.white)
                .keyboardType(.numberPad)
                .multilineTextAlignment(.leading)
                .fixedSize()
            }
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.top, 24)

            if let amountError = viewModel.amountError {
                Text(amountError)
                    .textStyle(.body14Regular())
                    .foregroundStyle(Color(.systemError))
                    .multilineTextAlignment(.center)
                    .padding(.top, 12)
            }

            HStack(spacing: 8) {
                ForEach(viewModel.quickAmounts) { quickAmount in
                    Button {
                        viewModel.onSelectQuickAmount?(quickAmount)
                    } label: {
                        Text(quickAmount.title)
                            .textStyle(.title18SemiBold())
                            .foregroundStyle(.white)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 12)
                            .background(Color(.white12), in: Capsule())
                    }
                }
            }
            .padding(.top, quickAmountsTopPadding)

            Spacer()

            // TODO: TBD if this is still needed.
//            Text(.FiatOnRamp.fiatOnrampDisclaimer)
//                .textStyle(.caption12Regular())
//                .foregroundStyle(.textAndIconsTertiaryDark)
//                .multilineTextAlignment(.center)
//                .padding(.bottom, 16)

            Button {
                viewModel.onContinue?(viewModel.amount)
            } label: {
                Text(.FiatOnRamp.fiatOnrampContinueButton)
                    .textStyle(.title16SemiBold())
                    .foregroundStyle(continueTextColor)
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, 32)
                    .padding(.vertical, 16)
                    .background(continueBackground, in: RoundedRectangle(cornerRadius: 16))
            }
            .disabled(!viewModel.isContinueEnabled)
            .padding(.bottom, 32)
        }
        .padding(.horizontal, 32)
        .padding(.top, 88)
        .background(Color(.backgroundPrimary))
    }
}

#Preview("Fiat On Ramp") {
    let viewModel = FiatOnRampViewModel()
    viewModel.amount = 120
    viewModel.quickAmounts = [
        .init(id: "50", value: 50, title: "$50"),
        .init(id: "100", value: 100, title: "$100"),
        .init(id: "200", value: 200, title: "$200")
    ]

    return FiatOnRampViewLayout(viewModel: viewModel)
        .background(Color(.backgroundPrimary))
}
