import DesignSystem
import SwiftUI

/// Shown while a payment waits for the installation's on-chain backup to complete.
struct PaymentsSetupNoticeView: View {
    let onDismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            ProgressView()
                .progressViewStyle(.circular)
                .controlSize(.small)
            Text(.paymentsSetupNoticeMessage)
                .typography(.bodyMedium)
                .foregroundStyle(Color.fgPrimary)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .foregroundStyle(Color.fgSecondary)
            }
            .accessibilityLabel(Text(.paymentsSetupNoticeDismiss))
        }
        .padding(EdgeInsets(top: 16, leading: 20, bottom: 16, trailing: 20))
        .background(Color.bgSurfaceContainer)
        .overlay(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .stroke(Color.strokePrimary, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
    }
}
