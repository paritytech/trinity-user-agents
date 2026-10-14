import SwiftUI
import DesignSystem

public enum UsernameAvailabilityViewModel {
    case available
    case taken
    case invalid
}

struct UsernameAvailabilityLabel: View {
    let viewModel: UsernameAvailabilityViewModel

    var body: some View {
        Text(text)
            .typography(.paragraphSmall)
            .foregroundStyle(color)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(backgroundColor, in: RoundedRectangle(cornerRadius: 8))
    }

    private var text: String {
        switch viewModel {
        case .available: String(localized: .claimUsernameIsAvailable)
        case .taken: String(localized: .claimUsernameIsTaken)
        case .invalid: String(localized: .claimUsernameIsInvalid)
        }
    }

    private var color: Color {
        switch viewModel {
        case .available: Color.fgSuccess
        case .taken,
             .invalid: Color.fgError
        }
    }

    private var backgroundColor: Color {
        switch viewModel {
        case .available: Color.bgStatusSuccess.opacity(0.12)
        case .taken,
             .invalid: Color.bgStatusError.opacity(0.12)
        }
    }
}
