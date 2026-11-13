import DesignSystem
import PolkadotUI
import SwiftUI

/// The title row every funding screen opens with: a round back button, the
/// title centred over the sheet, and room for one trailing control.
struct FundingScreenHeader<Trailing: View>: View {
    let title: String
    var leadingTitle = false
    var onBack: (() -> Void)?
    @ViewBuilder var trailing: () -> Trailing

    var body: some View {
        ZStack {
            Text(title)
                .typography(leadingTitle ? .titleSmall : .titleMedium)
                .foregroundStyle(.fgPrimary)
                .frame(maxWidth: .infinity, alignment: leadingTitle ? .leading : .center)
                .padding(.horizontal, onBack == nil ? 0 : 48)

            HStack {
                if let onBack {
                    Button(action: onBack) {
                        Image(systemName: "chevron.left")
                            .font(.system(size: 16, weight: .semibold))
                    }
                    .buttonStyle(.dsIcon(style: .secondary, shape: .pill, size: .medium))
                }
                Spacer()
                trailing()
            }
        }
        .frame(height: 44)
    }
}

extension FundingScreenHeader where Trailing == EmptyView {
    init(title: String, leadingTitle: Bool = false, onBack: (() -> Void)?) {
        self.title = title
        self.leadingTitle = leadingTitle
        self.onBack = onBack
        trailing = { EmptyView() }
    }
}

/// One label and value row of a summary.
struct FundingValueRow<Value: View>: View {
    let title: String
    var action: (() -> Void)?
    @ViewBuilder var value: () -> Value

    var body: some View {
        Button {
            action?()
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: DSSpacings.small) {
                Text(title)
                    .typography(.bodyLarge)
                    .foregroundStyle(.fgSecondary)
                Spacer(minLength: DSSpacings.small)
                value()
                if action != nil {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.fgPrimary)
                }
            }
            .padding(.vertical, DSSpacings.small)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(action == nil)
    }
}

/// A summary row whose value the user can copy, saying so in a toast.
struct FundingCopyRow: View {
    let title: String
    let text: String
    @Binding var copied: String?
    /// One line, cut in the middle, for an id only support reads in full.
    var truncatesMiddle = false
    /// What is drawn in place of `text`, such as a shortened address; the
    /// copy is still the whole `text`.
    var shown: String?

    var body: some View {
        FundingValueRow(title: title) {
            HStack(spacing: DSSpacings.small) {
                Text(verbatim: shown ?? text)
                    .typography(.bodyLargeEmphasized)
                    .foregroundStyle(.fgPrimary)
                    .multilineTextAlignment(.trailing)
                    .lineLimit(truncatesMiddle ? 1 : nil)
                    .truncationMode(.middle)
                Button {
                    UIPasteboard.general.string = text
                    copied = String(localized: .Funding.depositCopied)
                } label: {
                    Image(systemName: "doc.on.doc")
                        .foregroundStyle(.fgPrimary)
                }
                .buttonStyle(.plain)
            }
        }
    }
}

extension DesignSystem.TypographyStyle {
    /// A large amount sized by how many digits it has, smaller as it grows.
    static func fundingFigure(digits count: Int) -> DesignSystem.TypographyStyle {
        switch count {
        case 0 ... 3: .displayLarge
        case 4 ... 5: .displayMedium
        default: .displaySmall
        }
    }
}

/// A provider's icon from its root manifest, or its first letter.
struct FundingProviderLogo: View {
    let brand: FundingProviderBrand
    var size: DSLetterAvatar.Size = .s40

    var body: some View {
        if let icon = brand.icon {
            Image(uiImage: icon)
                .resizable()
                .scaledToFill()
                .frame(width: size.dimension, height: size.dimension)
                .clipShape(Circle())
        } else {
            DSLetterAvatar(
                letter: String(brand.name.prefix(1)).uppercased(),
                background: .bgSurfaceNested,
                foreground: .fgPrimary,
                size: size
            )
        }
    }
}

/// A grey bar standing in for text that is still loading.
struct FundingSkeleton: View {
    var width: CGFloat = 80
    var height: CGFloat = 14

    var body: some View {
        RoundedRectangle(cornerRadius: height / 2)
            .fill(Color.bgSurfaceNested)
            .frame(width: width, height: height)
    }
}

extension View {
    /// Shows `message` as the app's toast over the screen on top, then clears
    /// it, so the same message can be shown again.
    func fundingToast(_ message: Binding<String?>) -> some View {
        onChange(of: message.wrappedValue) { _, text in
            guard let text else { return }
            UIWindow.topWindow?.rootViewController?.topModalViewController.showToast(
                message: text,
                type: .success
            )
            message.wrappedValue = nil
        }
    }
}
