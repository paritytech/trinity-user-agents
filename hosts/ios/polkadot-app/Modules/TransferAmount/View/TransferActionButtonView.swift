import SwiftUI
import Combine
import ExternalAccessibility
import PolkadotUI
import DesignSystem

final class TransferActionButtonModel: ObservableObject {
    @Published var title: String = ""
    @Published var isLoading: Bool = false
    @Published var isActive: Bool = true
    @Published var statusText: String?
    var action: () -> Void = {}
}

typealias TransferActionButtonController = UIHostingController<TransferActionButtonView>

struct TransferActionButtonView: View {
    static let size: DSButtonStyle.Size = .mediumIncreased

    @ObservedObject var model: TransferActionButtonModel

    var body: some View {
        Group {
            if model.isLoading {
                Capsule()
                    .fill(Color.bgActionPrimary)
                    .overlay {
                        HStack(spacing: 8) {
                            LoadingSpinner(
                                lineWidth: 3,
                                strokeStyle: .fgPrimaryInverted
                            )
                            .frame(width: 20, height: 20)
                            if let text = model.statusText {
                                Text(text)
                                    .typography(.titleMedium.emphasized)
                                    .foregroundColor(.fgPrimaryInverted)
                            }
                        }
                    }
                    .frame(height: Self.size.height)
                    .disabled(true)
            } else {
                DSButton(
                    model.title,
                    style: .primary,
                    shape: .pill,
                    size: Self.size,
                    leadingIcon: model.isActive ? .iconArrowUp16 : nil,
                    expands: true,
                    action: model.action
                )
                .disabled(!model.isActive)
            }
        }
        .accessibilityId(AccessibilityID.TransferAmount.submitButton)
    }
}
