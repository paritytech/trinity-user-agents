import DesignSystem
import PolkadotUI
import SwiftUI

struct ProductUpdateNoticeView: View {
    let productName: String
    let icon: any ImageViewModelProtocol
    let onUpdate: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: DSSpacings.small) {
            AsyncImageView(viewModel: icon)
                .frame(width: 24, height: 24)
                .clipShape(RoundedRectangle(cornerRadius: DSRadii.extraSmall, style: .continuous))
            Text(.Products.updateNoticeMessage(product: productName))
                .typography(.bodySmall)
                .foregroundStyle(Color.fgPrimary)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button(action: onUpdate) {
                Text(.Products.updateNoticeAction)
            }
            .buttonStyle(.ds(style: .primary, shape: .pill, size: .small))
            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .font(.system(size: 16, weight: .medium))
                    .foregroundStyle(Color.fgSecondary)
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .accessibilityLabel(Text(.Common.close))
        }
        .padding(EdgeInsets(
            top: DSSpacings.extraSmall,
            leading: DSSpacings.small,
            bottom: DSSpacings.extraSmall,
            trailing: DSSpacings.extraSmall
        ))
        .frame(minHeight: 44)
        .background(
            Color.bgSurfaceContainer,
            in: RoundedRectangle(cornerRadius: DSRadii.medium, style: .continuous)
        )
    }
}
