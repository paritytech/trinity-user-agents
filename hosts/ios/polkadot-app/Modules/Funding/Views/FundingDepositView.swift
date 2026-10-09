import CoreImage.CIFilterBuiltins
import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Where to send the funds, as the provider reported it, while the session
/// waits for them.
struct FundingDepositView: View {
    let model: FundingFlowModel

    @State private var copied: String?

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: title, onBack: model.hasStarted ? model.showProgress : model.back)

            ScrollView {
                content
            }

            if model.failure != nil {
                FundingPrimaryButton(title: String(localized: .Funding.failureClose), action: model.close)
            } else if model.hasStarted {
                HStack(spacing: DSSpacings.small) {
                    FundingPrimaryButton(
                        title: String(localized: .Funding.depositCancel),
                        style: .destructive,
                        action: model.confirmCancel
                    )
                    waitingPill
                }
            }
        }
        .fundingToast($copied)
        .task { await follow() }
    }
}

private extension FundingDepositView {
    var title: String {
        switch model.rail {
        case .card: String(localized: .Funding.summaryCardTitle)
        case .bank: String(localized: .Funding.summaryBankTitle)
        case .crypto: String(localized: .Funding.summaryCryptoTitle)
        }
    }

    @ViewBuilder
    var content: some View {
        if let failure = model.failure {
            errorText(failure.failedText)
        } else if let deposit = model.progress?.deposit {
            switch deposit {
            case let .crypto(address, network, asset, amount, decimals, exact, uri, _):
                cryptoDeposit(
                    address: address,
                    network: network,
                    amount: FundingAssetUnit(code: asset, decimals: decimals).format(amount),
                    exact: exact,
                    uri: uri
                )
            case let .bank(amount, currency, decimals, beneficiary, account, bankCode, reference, _):
                VStack(spacing: DSSpacings.small) {
                    copyRow(
                        String(localized: .Funding.depositExactAmount),
                        FundingAssetUnit(code: currency, decimals: decimals).format(amount)
                    )
                    beneficiary.map { copyRow(String(localized: .Funding.depositBeneficiary), $0) }
                    account.map { copyRow(String(localized: .Funding.depositAccount), $0) }
                    bankCode.map { copyRow(String(localized: .Funding.depositBankCode), $0) }
                    copyRow(String(localized: .Funding.depositReference), reference)
                }
            }
        } else if let startError = model.startError {
            errorText(startError)
        } else if model.noProviderQuoted, !model.hasStarted {
            errorText(model.quoteFailureText ?? String(localized: .Funding.errorNoProvider))
        } else {
            VStack(spacing: DSSpacings.medium) {
                ProgressView().tint(.fgPrimary)
                Text(pendingText)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgSecondary)
            }
            .padding(.top, DSSpacings.extraLarge)
        }
    }

    var pendingText: LocalizedStringResource {
        guard model.hasStarted else { return .Funding.depositFindingProvider }
        return model.rail == .card ? .Funding.depositCardInProgress : .Funding.depositPreparing
    }

    func errorText(_ text: String) -> some View {
        Text(verbatim: text)
            .typography(.bodyMedium)
            .foregroundStyle(.fgError)
            .multilineTextAlignment(.center)
            .padding(.top, DSSpacings.large)
    }

    func cryptoDeposit(address: String, network: String, amount: String, exact: Bool, uri: String?) -> some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingQRCode(payload: uri ?? address)
                .frame(width: 220, height: 220)
                .padding(DSSpacings.medium)
                .background(.white, in: RoundedRectangle(cornerRadius: DSRadii.large))

            VStack(spacing: DSSpacings.small) {
                if exact {
                    copyRow(String(localized: .Funding.depositExactAmount), amount)
                } else {
                    copyRow(String(localized: .Funding.depositAtLeast), amount)
                }
                copyRow(
                    String(localized: .Funding.depositAddressOn(network: FundingNetwork(id: network).name)),
                    address,
                    shown: Self.shortened(address)
                )
            }
        }
    }

    func copyRow(_ title: String, _ value: String, shown: String? = nil) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: title)
                    .typography(.captionSmall)
                    .foregroundStyle(.fgSecondary)
                Text(verbatim: shown ?? value)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgPrimary)
            }
            Spacer()
            Button {
                UIPasteboard.general.string = value
                copied = String(localized: .Funding.depositCopied)
            } label: {
                Image(systemName: "doc.on.doc")
                    .foregroundStyle(.fgPrimary)
            }
        }
        .padding(.vertical, DSSpacings.small)
        .overlay(alignment: .bottom) { Divider().overlay(Color.strokePrimary) }
    }

    /// Opens the progress screen the deposit sits over.
    var waitingPill: some View {
        Button(action: model.showProgress) {
            HStack(spacing: DSSpacings.extraSmall) {
                ProgressView().controlSize(.small).tint(.fgPrimary)
                Text(.Funding.depositWaiting)
                    .typography(.labelMedium)
                    .foregroundStyle(.fgPrimary)
            }
            .frame(maxWidth: .infinity)
            .frame(height: 52)
            .background(.bgSurfaceNested, in: Capsule())
        }
        .buttonStyle(.plain)
    }

    /// Follows the session while the screen is up: the deposit can arrive or
    /// change, and once the funds are seen or the session ends the progress
    /// screen takes over.
    func follow() async {
        while !Task.isCancelled {
            if model.hasStarted {
                model.refreshSession()
                let ended = model.session.map { !$0.stage.isOpen } ?? false
                if ended || model.hasReached(.payment) {
                    model.showProgress()
                    return
                }
            }
            try? await Task.sleep(for: .seconds(3))
        }
    }

    static func shortened(_ address: String) -> String {
        guard address.count > 14 else { return address }
        return "\(address.prefix(6))…\(address.suffix(5))"
    }
}

/// A QR code for `payload`, drawn sharp at any size.
struct FundingQRCode: View {
    let payload: String

    var body: some View {
        if let image = Self.image(for: payload) {
            Image(uiImage: image)
                .interpolation(.none)
                .resizable()
                .scaledToFit()
        }
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

/// Asks before giving up on a deposit the user may already have paid.
struct FundingCancelConfirmView: View {
    let model: FundingFlowModel

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: "", onBack: model.back)

            Text(.Funding.cancelTitle)
                .typography(.headlineMedium)
                .foregroundStyle(.fgPrimary)
            Text(.Funding.cancelBody)
                .typography(.bodyMedium)
                .foregroundStyle(.fgPrimary)
                .multilineTextAlignment(.center)

            Spacer(minLength: 0)

            FundingPrimaryButton(
                title: String(localized: .Funding.depositCancel),
                style: .destructive,
                isLoading: model.isCancelling,
                action: model.cancelTopUp
            )
            FundingPrimaryButton(title: String(localized: .Funding.cancelKeep), action: model.back)
        }
    }
}
