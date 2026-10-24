import DesignSystem
import SwiftUI

struct ProductUpdateNoticeView: View {
    let productName: String
    let onUpdate: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            Text(.Products.updateNoticeMessage(product: productName))
                .typography(.bodyMedium)
                .foregroundStyle(Color.fgPrimary)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button(action: onUpdate) {
                Text(.Products.updateNoticeAction)
                    .typography(.bodyMedium)
                    .foregroundStyle(Color.fgPrimary)
            }
            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .foregroundStyle(Color.fgSecondary)
            }
            .accessibilityLabel(Text(.Common.close))
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
