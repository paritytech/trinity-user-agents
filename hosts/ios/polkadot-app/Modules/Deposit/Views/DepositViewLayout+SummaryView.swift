import SwiftUI
import PolkadotUI

extension DepositViewLayout {
    private struct OverallSectionView: View {
        let viewModel: DepositSummaryViewModel
        let onCopyAddress: (() -> Void)?

        var body: some View {
            VStack(spacing: 12) {
                HStack {
                    if let minimumAmount = viewModel.minimumAmount {
                        Text(.depositMinimumAmount(minimumAmount))
                            .textStyle(.body16Regular())
                            .foregroundStyle(.textAndIconsSecondary)
                    }

                    Spacer()

                    HStack(alignment: .center, spacing: 8) {
                        if let viewModel = viewModel.assetIcon {
                            AsyncImageView(
                                viewModel: viewModel,
                                settings: ImageViewModelSettings(targetSize: CGSize(width: 24, height: 24))
                            )
                            .frame(width: 24, height: 24, alignment: .leading)
                        }

                        Text(viewModel.asset)
                            .textStyle(.title16SemiBold())
                            .foregroundStyle(.textAndIconsPrimaryDark)
                    }
                    .padding([.vertical, .leading], 4)
                    .padding(.trailing, 12)
                    .layoutPriority(1)
                    .background(.fill12, in: Capsule())
                }

                HStack(spacing: 0) {
                    Text(.depositToTitle)
                        .textStyle(.body16Regular())
                        .foregroundStyle(.textAndIconsSecondary)
                        .layoutPriority(1)

                    Spacer(minLength: 16)

                    Button(
                        action: {
                            onCopyAddress?()
                        }, label: {
                            HStack(alignment: .center, spacing: 8) {
                                Text(viewModel.address)
                                    .textStyle(.body16Regular())
                                    .foregroundStyle(.textAndIconsPrimaryDark)
                                    .truncationMode(.middle)
                                    .lineLimit(1)

                                Image(.depositToCopy)
                            }
                        }
                    )
                }

                HStack(spacing: 0) {
                    Text(.depositNetworkTitle)
                        .textStyle(.body16Regular())
                        .foregroundStyle(.textAndIconsSecondary)
                        .layoutPriority(1)

                    Spacer(minLength: 16)

                    Button(
                        action: {}, label: {
                            HStack(alignment: .center, spacing: 8) {
                                Text(viewModel.network)
                                    .textStyle(.body16Regular())
                                    .foregroundStyle(.textAndIconsPrimaryDark)
                                Image(.depositNetworkInfo)
                            }
                        }
                    )
                }
            }
        }
    }

    private struct QRCodeSectionView: View {
        let viewModel: DepositSummaryViewModel
        let onCopyAddress: (() -> Void)?

        var body: some View {
            VStack(alignment: .center, spacing: 24) {
                // QR code
                Group {
                    if let qrCode = viewModel.qrCodeImage {
                        Image(uiImage: qrCode).resizable()
                    } else {
                        Rectangle()
                            .foregroundStyle(.white100)
                    }
                }
                .aspectRatio(1, contentMode: .fit)
                .padding(8)
                .frame(height: 230) // TODO: Pass outside
                .background(
                    .white100,
                    in: RoundedRectangle(cornerRadius: 20)
                )

                Text(viewModel.address)
                    .textStyle(.body16Regular())
                    .foregroundStyle(.textAndIconsPrimaryDark)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 40)

                // Action
                Button {
                    onCopyAddress?()
                } label: {
                    Text(.depositCopyAction)
                        .textStyle(.title16SemiBold())
                        .padding(.horizontal, 32)
                        .padding(.vertical, 12)
                }
                .buttonStyle(.mainWhite)
                .padding(.horizontal, 8)
            }
        }
    }

    private struct FeeRateView: View {
        let viewModel: DepositSummaryViewModel

        var body: some View {
            VStack(spacing: 12) {
                HStack {
                    Text(.depositRateTitle)
                        .textStyle(.body16Regular())
                        .foregroundStyle(.textAndIconsSecondary)
                        .layoutPriority(1)

                    Spacer(minLength: 16)

                    Text("\(viewModel.rateAmountIn) ≈ \(viewModel.rateAmountOut)")
                        .textStyle(.body16Regular())
                        .foregroundStyle(.textAndIconsPrimaryDark)
                        .lineLimit(1)
                }

                HStack {
                    Text(.feeTitle)
                        .textStyle(.body16Regular())
                        .foregroundStyle(.textAndIconsSecondary)
                        .layoutPriority(1)

                    Spacer(minLength: 16)

                    Text("≈ \(viewModel.fee)")
                        .textStyle(.body16Regular())
                        .foregroundStyle(.textAndIconsPrimaryDark)
                        .lineLimit(1)
                }
            }
        }
    }

    struct SummaryView: View {
        let viewModel: DepositSummaryViewModel
        let onCopyAddress: (() -> Void)?

        var body: some View {
            VStack {
                OverallSectionView(
                    viewModel: viewModel,
                    onCopyAddress: onCopyAddress
                )
                .padding(24)

                Divider()
                    .background(.textAndIconsSecondary)
                    .padding(.horizontal, 24)

                QRCodeSectionView(
                    viewModel: viewModel,
                    onCopyAddress: onCopyAddress
                )
                .padding(.bottom, 24)
                .padding(.top, 24)

                Divider()
                    .background(.textAndIconsSecondary)
                    .padding(.horizontal, 24)

                FeeRateView(viewModel: viewModel)
                    .padding(24)
            }
            .frame(maxWidth: .infinity)
            .background(
                .white8,
                in: RoundedRectangle(cornerRadius: 24)
            )
        }
    }
}
