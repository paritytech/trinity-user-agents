import CoreImage.CIFilterBuiltins
import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Where to send the funds, as the provider reported it, while the session
/// waits for them.
struct FundingDepositView: View {
    let viewModel: FundingDepositViewModel
    let presenter: FundingDepositPresenterProtocol


    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: viewModel.title, onBack: presenter.back)

            ScrollView {
                content
            }

            switch viewModel.action {
            case .close:
                DSButton(.Funding.failureClose, expands: true, action: presenter.close)
            case .cancel:
                HStack(spacing: DSSpacings.small) {
                    DSButton(
                        .Funding.depositCancel,
                        style: .destructive,
                        expands: true,
                        action: presenter.confirmCancel
                    )
                    waitingPill
                }
            case .none:
                EmptyView()
            }
        }
        .onChange(of: viewModel.paymentSeen, initial: true) { _, seen in
            if seen { presenter.showProgress() }
        }
        .fundingScreen()
    }
}

private extension FundingDepositView {
    @ViewBuilder
    var content: some View {
        switch viewModel.content {
        case let .error(text):
            errorText(text)
        case let .crypto(deposit):
            cryptoDeposit(deposit)
        case let .bank(lines):
            VStack(spacing: DSSpacings.small) {
                ForEach(lines, id: \.title) { line in
                    copyRow(line.title, line.text)
                }
            }
        case let .pending(text):
            VStack(spacing: DSSpacings.medium) {
                ProgressView().tint(.fgPrimary)
                Text(text)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgSecondary)
            }
            .padding(.top, DSSpacings.extraLarge)
        }
    }

    func errorText(_ text: String) -> some View {
        Text(verbatim: text)
            .typography(.bodyMedium)
            .foregroundStyle(.fgError)
            .multilineTextAlignment(.center)
            .padding(.top, DSSpacings.large)
    }

    func cryptoDeposit(_ deposit: FundingDepositViewModel.Crypto) -> some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingQRCode(payload: deposit.payload)
                .frame(width: 220, height: 220)
                .padding(DSSpacings.medium)
                .background(.white, in: RoundedRectangle(cornerRadius: DSRadii.large))

            VStack(spacing: DSSpacings.small) {
                if let amount = deposit.amount {
                    copyRow(deposit.amountTitle, amount)
                }
                copyRow(deposit.addressTitle, deposit.address, shown: Self.shortened(deposit.address))
            }
        }
    }

    func copyRow(_ title: String, _ value: String, shown: String? = nil) -> some View {
        FundingCopyRow(title: title, text: value, onCopy: presenter.didCopy, shown: shown)
            .padding(.vertical, DSSpacings.small)
            .overlay(alignment: .bottom) { Divider().overlay(Color.strokePrimary) }
    }

    /// Opens the progress screen the deposit sits over.
    var waitingPill: some View {
        Button(action: presenter.showProgress) {
            HStack(spacing: DSSpacings.extraSmall) {
                ProgressView().controlSize(.small).tint(.fgPrimary)
                Text(.Funding.depositWaiting)
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.ds(style: .tertiary, shape: .pill, size: .large))
    }

    static func shortened(_ address: String) -> String {
        guard address.count > 14 else { return address }
        return "\(address.prefix(6))…\(address.suffix(5))"
    }
}

/// A QR code for `payload`, drawn sharp at any size. Each payload's image is
/// built once, not on every redraw of the deposit screen.
struct FundingQRCode: View {
    let payload: String

    private static let images = NSCache<NSString, UIImage>()

    var body: some View {
        if let image = Self.cachedImage(for: payload) {
            Image(uiImage: image)
                .interpolation(.none)
                .resizable()
                .scaledToFit()
        }
    }

    static func cachedImage(for payload: String) -> UIImage? {
        if let image = images.object(forKey: payload as NSString) { return image }
        guard let image = image(for: payload) else { return nil }

        images.setObject(image, forKey: payload as NSString)
        return image
    }

    static func image(for payload: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(payload.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage,
              let cgImage = CIContext().createCGImage(output, from: output.extent) else { return nil }
        return UIImage(cgImage: cgImage)
    }
}
