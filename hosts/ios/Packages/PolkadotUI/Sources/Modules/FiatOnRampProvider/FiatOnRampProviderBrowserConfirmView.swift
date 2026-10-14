import SwiftUI

public struct FiatOnRampProviderBrowserConfirmView: View {
    let title: String
    let message: String
    let onCancel: () -> Void
    let onConfirm: () -> Void

    public init(
        title: String,
        message: String,
        onCancel: @escaping () -> Void,
        onConfirm: @escaping () -> Void
    ) {
        self.title = title
        self.message = message
        self.onCancel = onCancel
        self.onConfirm = onConfirm
    }

    public var body: some View {
        VStack(spacing: 16) {
            Text(title)
                .textStyle(.title24SemiBold())
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Text(message)
                .textStyle(.body14Regular())
                .foregroundStyle(.white.opacity(0.48))
                .multilineTextAlignment(.center)

            HStack(spacing: 12) {
                Button(action: onCancel) {
                    Text(String(localized: .FiatOnRamp.fiatOnrampBrowserAlertCancel))
                        .textStyle(.title18SemiBold())
                        .foregroundStyle(.white)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 16)
                        .background(Color(.fill6), in: RoundedRectangle(cornerRadius: 16))
                }

                Button(action: onConfirm) {
                    Text(String(localized: .FiatOnRamp.fiatOnrampBrowserAlertConfirm))
                        .textStyle(.title18SemiBold())
                        .foregroundStyle(.black)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 16)
                        .background(.white, in: RoundedRectangle(cornerRadius: 16))
                }
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.fill12), in: RoundedRectangle(cornerRadius: 24))
        .padding(.horizontal, 16)
    }
}

#Preview("Browser Confirm") {
    FiatOnRampProviderBrowserConfirmView(
        title: "Continue In Browser?",
        message: "You will be redirected to example.com to finish your purchase.",
        onCancel: {},
        onConfirm: {}
    )
    .background(Color(.backgroundPrimary))
}
