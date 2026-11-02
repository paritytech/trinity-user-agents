import DesignSystem
import PolkadotUI
import SwiftUI

struct ProductUpdateNoticeView: View {
    private static let iconSize: CGFloat = 24

    let productName: String
    let icon: any ImageViewModelProtocol
    let onUpdate: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: DSSpacings.small) {
            // A UIImageView takes its loaded image's size, so the image is processed to the frame it fills.
            AsyncImageView(
                viewModel: icon,
                settings: ImageViewModelSettings(
                    targetSize: CGSize(width: Self.iconSize, height: Self.iconSize),
                    cornerRadius: DSRadii.extraSmall
                )
            )
            .frame(width: Self.iconSize, height: Self.iconSize)
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
