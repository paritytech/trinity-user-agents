import DesignSystem
import SwiftUI

public struct StallBannerView: View {
    private static let indicatorSize: CGFloat = 14

    // `.controlSize(.mini)` is the smallest discrete size SwiftUI offers for a circular
    // ProgressView, but it still doesn't match `indicatorSize`. `.frame` alone cannot shrink
    // a ProgressView because its intrinsic content size ignores the frame proposal, so we
    // scale it down with `.scaleEffect` and reserve the smaller layout box explicitly.
    private static let miniProgressViewSize: CGFloat = 16
    private static let depthIndent: CGFloat = 12

    public let viewModel: StallBannerViewModel
    public let onDismiss: (UUID) -> Void

    public init(viewModel: StallBannerViewModel, onDismiss: @escaping (UUID) -> Void) {
        self.viewModel = viewModel
        self.onDismiss = onDismiss
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(viewModel.transactions, id: \.id) { transaction in
                transactionView(transaction)
            }
            if let overflowText = viewModel.overflowText {
                overflowView(overflowText)
            }
        }
        .padding(EdgeInsets(top: 16, leading: 24, bottom: 16, trailing: 24))
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.bgSurfaceContainer)
        .overlay(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .stroke(Color.strokePrimary, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
    }

    private func transactionView(_ transaction: StallBannerViewModel.Transaction) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                if let chainName = transaction.chainName {
                    Text(chainName)
                        .typography(.titleSmall)
                        .foregroundStyle(Color.fgSecondary)
                        .lineLimit(1)
                        .layoutPriority(1)
                        .fixedSize(horizontal: true, vertical: false)
                }
                Text(transaction.title)
                    .typography(.titleSmall)
                    .foregroundStyle(Color.fgSecondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button {
                    onDismiss(transaction.id)
                } label: {
                    Image(systemName: "xmark")
                        .foregroundStyle(Color.fgSecondary)
                }
                .accessibilityLabel(transaction.dismissAccessibilityLabel ?? "")
            }
            VStack(alignment: .leading, spacing: 4) {
                ForEach(transaction.stages, id: \.id) { stage in
                    stageView(stage)
                }
            }
            .padding(.leading, 12)
        }
    }

    private func stageView(_ stage: StallBannerViewModel.Stage) -> some View {
        HStack(spacing: 8) {
            indicator(for: stage.state)
                .frame(width: Self.indicatorSize, height: Self.indicatorSize)
            VStack(alignment: .leading, spacing: 2) {
                Text(stage.text)
                    .typography(.titleSmall)
                    .foregroundStyle(textColor(for: stage.state))
                    .lineLimit(1)
                if let detail = stage.detail {
                    Text(detail)
                        .typography(.bodySmall)
                        .foregroundStyle(Color.fgSecondary)
                        .lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                        .multilineTextAlignment(.leading)
                }
            }
        }
        .padding(.leading, CGFloat(stage.depth) * Self.depthIndent)
    }

    private func overflowView(_ text: String) -> some View {
        HStack(spacing: 8) {
            Color.clear
                .frame(width: Self.indicatorSize, height: Self.indicatorSize)
            Text(text)
                .typography(.titleMedium)
                .foregroundStyle(Color.fgPrimary)
                .lineLimit(1)
        }
    }

    private func textColor(for state: StallBannerViewModel.Stage.State) -> Color {
        switch state {
        case .done,
             .current,
             .failed: Color.fgPrimary
        case .upcoming,
             .skipped: Color.fgSecondary
        }
    }

    @ViewBuilder
    private func indicator(for state: StallBannerViewModel.Stage.State) -> some View {
        switch state {
        case .done:
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: Self.indicatorSize))
                .foregroundStyle(Color.fgSuccess)
        case .current:
            ProgressView()
                .progressViewStyle(.circular)
                .controlSize(.mini)
                .scaleEffect(Self.indicatorSize / Self.miniProgressViewSize)
                .frame(width: Self.indicatorSize, height: Self.indicatorSize)
        case .upcoming:
            Image(systemName: "circle")
                .font(.system(size: Self.indicatorSize))
                .foregroundStyle(Color.fgSecondary)
        case .skipped:
            Image(systemName: "xmark")
                .font(.system(size: Self.indicatorSize))
                .foregroundStyle(Color.fgSecondary)
        case .failed:
            Image(systemName: "exclamationmark.circle")
                .font(.system(size: Self.indicatorSize))
                .foregroundStyle(Color.fgPrimary)
        }
    }
}

#if DEBUG
    #Preview("StallBannerView") {
        StallBannerView(
            viewModel: .init(
                transactions: [
                    .init(
                        id: UUID(),
                        title: "0x1f4a3f7c9e2b6d0148a5f3c7e9b2d6a04871c3e5f9b2d6a04871c3e5f9b2d5c2b",
                        chainName: "Polkadot",
                        stages: [
                            .init(id: "submitting", text: "Submitting", state: .done),
                            .init(id: "broadcasting", text: "Broadcasting", state: .done),
                            .init(id: "inBlock", text: "In block", state: .current)
                        ]
                    ),
                    .init(
                        id: UUID(),
                        title: "Transaction",
                        stages: [
                            .init(id: "submitting", text: "Submitting", state: .current),
                            .init(id: "broadcasting", text: "Broadcasting", state: .upcoming),
                            .init(id: "inBlock", text: "In block", state: .upcoming)
                        ]
                    ),
                    .init(
                        id: UUID(),
                        title: "0x9e2a1d8b4f6c0a37e5d9b1f4a6c8e0d2b4f6a8c0e2d4b6f8a0c2e4d6b8f044f1",
                        stages: [
                            .init(
                                id: "reserving",
                                depth: 0,
                                text: "Reserving a slot",
                                state: .done
                            ),
                            .init(
                                id: "readingState",
                                depth: 1,
                                text: "Reading chain state",
                                state: .done
                            ),
                            .init(
                                id: "submitting",
                                depth: 1,
                                text: "Submitting",
                                state: .done
                            ),
                            .init(
                                id: "sendingStatement",
                                depth: 0,
                                text: "Sending statement",
                                state: .current
                            )
                        ],
                        dismissAccessibilityLabel: "Dismiss"
                    )
                ],
                overflowText: "+2 more"
            ),
            onDismiss: { _ in }
        )
        .padding()
        .background(Color.bgSurfaceMain)
    }
#endif
