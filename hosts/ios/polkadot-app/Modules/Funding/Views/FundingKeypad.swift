import DesignSystem
import SwiftUI

/// Digits, a decimal point and delete, editing a dollar amount of at most two
/// decimals.
struct FundingKeypad: View {
    @Binding var text: String
    var maximumIntegerDigits = 7

    private let keys: [[Key]] = [
        [.digit("1"), .digit("2"), .digit("3")],
        [.digit("4"), .digit("5"), .digit("6")],
        [.digit("7"), .digit("8"), .digit("9")],
        [.point, .digit("0"), .delete]
    ]

    var body: some View {
        VStack(spacing: DSSpacings.small) {
            ForEach(keys.indices, id: \.self) { row in
                HStack(spacing: DSSpacings.small) {
                    ForEach(keys[row], id: \.self) { key in
                        keyButton(key)
                    }
                }
            }
        }
    }

    enum Key: Hashable {
        case digit(String)
        case point
        case delete
    }

    static func apply(_ key: Key, to text: String, maximumIntegerDigits: Int = 7) -> String {
        switch key {
        case .delete:
            return String(text.dropLast())
        case .point:
            guard !text.contains(".") else { return text }
            return (text.isEmpty ? "0" : text) + "."
        case let .digit(digit):
            let parts = text.split(separator: ".", omittingEmptySubsequences: false)
            if parts.count == 2 {
                return parts[1].count < 2 ? text + digit : text
            }
            if text == "0" { return digit }
            return text.count < maximumIntegerDigits ? text + digit : text
        }
    }
}

private extension FundingKeypad {
    func keyButton(_ key: Key) -> some View {
        Button {
            text = Self.apply(key, to: text, maximumIntegerDigits: maximumIntegerDigits)
        } label: {
            label(for: key)
                .frame(maxWidth: .infinity)
                .frame(height: 48)
                .background(.bgSurfaceContainer, in: Capsule())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    func label(for key: Key) -> some View {
        switch key {
        case let .digit(digit):
            Text(verbatim: digit)
                .typography(.titleLarge)
                .foregroundStyle(.fgPrimary)
        case .point:
            Text(verbatim: ".")
                .typography(.titleLarge)
                .foregroundStyle(.fgPrimary)
        case .delete:
            Image(systemName: "delete.left")
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(.fgPrimary)
        }
    }
}
