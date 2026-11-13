import DesignSystem
import PolkadotUI
import SwiftUI
import UIKit

/// Tells the user a provider wants to show one of its screens for a session
/// that is not on screen, and opens it on Show.
@MainActor
protocol FundingFrameNoticePresenting: AnyObject {
    func show(brand: FundingProviderBrand, onShow: @escaping () -> Void)
    func hide()
}

@MainActor
final class FundingFrameNoticePresenter: FundingFrameNoticePresenting {
    private let application: UIApplication
    private var window: TopNoticeWindow?

    init(application: UIApplication) {
        self.application = application
    }

    func show(brand: FundingProviderBrand, onShow: @escaping () -> Void) {
        guard let windowScene = foregroundWindowScene() else { return }

        let notice = window ?? TopNoticeWindow(windowScene: windowScene)
        window = notice
        notice.show(FundingFrameNoticeView(
            brand: brand,
            onShow: { [weak self] in
                self?.hide()
                onShow()
            },
            onDismiss: { [weak self] in self?.hide() }
        ))
    }

    func hide() {
        window?.hide()
    }
}

private extension FundingFrameNoticePresenter {
    func foregroundWindowScene() -> UIWindowScene? {
        let scenes = application.connectedScenes.compactMap { $0 as? UIWindowScene }
        return scenes.first { $0.activationState == .foregroundActive } ?? scenes.first
    }
}

/// The notice, in the style of the app's product update notice.
struct FundingFrameNoticeView: View {
    let brand: FundingProviderBrand
    let onShow: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: DSSpacings.small) {
            FundingProviderLogo(brand: brand, size: .s28)
            Text(.Funding.providerFrameRequested(provider: brand.name))
                .typography(.bodySmall)
                .foregroundStyle(Color.fgPrimary)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button(action: onShow) {
                Text(.Funding.providerFrameShow)
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
