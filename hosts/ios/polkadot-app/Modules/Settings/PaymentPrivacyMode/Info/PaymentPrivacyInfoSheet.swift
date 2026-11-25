import SwiftUI
import DesignSystem
import PolkadotUI

/// Explains coin quality in two pages: what it is, with an illustration, then why it matters. Back on the
/// second page returns to the first; the dismiss button closes the sheet from either.
///
/// The page text scrolls when it outgrows the sheet (large Dynamic Type sizes, small screens) so the
/// buttons below always stay on screen.
///
/// The autosized sheet measures its content at the smallest height it will take, which a scroll view
/// answers with zero. So the sheet reports its natural height through ``onHeightChange`` instead: its
/// laid-out height with the scroll view swapped for the full page it scrolls. The sheet caps that at the
/// screen, and the scroll view takes whatever is left above the buttons.
struct PaymentPrivacyInfoSheet: View {
    enum Page {
        case quality
        case whyItMatters
    }

    var onDismiss: () -> Void = {}
    var onHeightChange: (CGFloat) -> Void = { _ in }

    @State private var page: Page = .quality
    @State private var pageHeight: CGFloat = 0
    @State private var scrollHeight: CGFloat = 0
    @State private var sheetHeight: CGFloat = 0

    var body: some View {
        BottomSheetBaseView {
            VStack(spacing: DSSpacings.large) {
                ScrollView(.vertical) {
                    pageContent
                        .onGeometryChange(for: CGFloat.self, of: \.size.height) { pageHeight = $0 }
                }
                .scrollBounceBehavior(.basedOnSize)
                .frame(maxHeight: pageHeight)
                .onGeometryChange(for: CGFloat.self, of: \.size.height) { scrollHeight = $0 }

                buttons
            }
        }
        .onGeometryChange(for: CGFloat.self, of: \.size.height) { sheetHeight = $0 }
        .frame(maxHeight: .infinity, alignment: .bottom)
        .onChange(of: naturalHeight) { _, height in onHeightChange(height) }
        .onChange(of: page) { _, _ in AccessibilityNotification.ScreenChanged().post() }
    }

    private var naturalHeight: CGFloat {
        (sheetHeight - scrollHeight + pageHeight).rounded(.up)
    }
}

// MARK: - Pages

private extension PaymentPrivacyInfoSheet {
    var pageContent: some View {
        Group {
            switch page {
            case .quality: qualityPage
            case .whyItMatters: whyItMattersPage
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, DSSpacings.small)
        .padding(.top, DSSpacings.small)
        .transition(.opacity)
    }

    var qualityPage: some View {
        VStack(alignment: .leading, spacing: DSSpacings.large) {
            title(.settingsPrivacymodeInfoQualityTitle)

            CoinQualityIllustration()

            VStack(alignment: .leading, spacing: DSSpacings.large) {
                paragraph(.settingsPrivacymodeInfoQualityParagraph1)
                paragraph(.settingsPrivacymodeInfoQualityParagraph2)
            }
        }
    }

    var whyItMattersPage: some View {
        VStack(alignment: .leading, spacing: DSSpacings.large) {
            // The navigation bar's back button, as in a chat: the same chevron, in system glass on
            // iOS 26 and bare below it.
            Button { show(.quality) } label: {
                Image(.buttonBack)
            }
            .buttonStyle(.dsIcon(style: .ghost, shape: .pill, size: .medium, glass: true))
            .accessibilityLabel(Text(.Common.back))

            title(.settingsPrivacymodeInfoMatterTitle)

            VStack(alignment: .leading, spacing: DSSpacings.large) {
                paragraph(.settingsPrivacymodeInfoMatterParagraph1)
                paragraph(.settingsPrivacymodeInfoMatterParagraph2)
            }
        }
    }

    func title(_ resource: LocalizedStringResource) -> some View {
        Text(resource)
            .typography(.headlineSmall)
            .foregroundStyle(.fgPrimary)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityAddTraits(.isHeader)
    }

    func paragraph(_ resource: LocalizedStringResource) -> some View {
        Text(Self.highlighted(resource))
            .typography(.paragraphLarge)
            .foregroundStyle(.fgSecondary)
            .fixedSize(horizontal: false, vertical: true)
    }

    /// The copy marks its key terms with Markdown `**…**`; they are set in the primary text colour at the
    /// paragraph's own weight, so the emphasis becomes a colour rather than bold.
    static func highlighted(_ resource: LocalizedStringResource) -> AttributedString {
        var text = AttributedString(localized: resource)
        let emphasized = text.runs
            .filter { $0.inlinePresentationIntent?.contains(.stronglyEmphasized) == true }
            .map(\.range)

        for range in emphasized {
            text[range].inlinePresentationIntent = nil
            text[range].swiftUI.foregroundColor = .fgPrimary
        }
        return text
    }
}

// MARK: - Buttons

private extension PaymentPrivacyInfoSheet {
    var buttons: some View {
        VStack(spacing: DSSpacings.small) {
            switch page {
            case .quality:
                DSButton(.Common.gotIt, size: .mediumIncreased, expands: true, action: onDismiss)
                DSButton(
                    .settingsPrivacymodeInfoMatterButton,
                    style: .secondary,
                    size: .mediumIncreased,
                    expands: true
                ) { show(.whyItMatters) }
            case .whyItMatters:
                DSButton(.Common.gotIt, style: .secondary, size: .mediumIncreased, expands: true, action: onDismiss)
            }
        }
    }

    func show(_ target: Page) {
        withAnimation(.easeInOut(duration: 0.2)) { page = target }
    }
}

// MARK: - Illustration

/// Three mismatched coins become three identical ones, going from lower to higher quality. The source
/// export ships with a sub-caption under each label; those lines are masked out of the asset.
private struct CoinQualityIllustration: View {
    var body: some View {
        Image(.coinQualityIllustration)
            .resizable()
            .scaledToFit()
            .clipShape(RoundedRectangle(cornerRadius: DSRadii.large, style: .continuous))
            .accessibilityLabel(Text(.settingsPrivacymodeInfoQualityIllustration))
    }
}

#if DEBUG
    #Preview("PaymentPrivacyInfoSheet") {
        PaymentPrivacyInfoSheet()
            .background(Color.bgSurfaceOverlay)
    }
#endif
