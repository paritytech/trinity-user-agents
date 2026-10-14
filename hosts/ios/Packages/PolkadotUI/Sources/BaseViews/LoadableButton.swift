import SwiftUI

public struct LoadableButtonConfiguration {
    public var isLoading: Bool
    public var isEnabled: Bool

    public init(
        isLoading: Bool = false,
        isEnabled: Bool = true
    ) {
        self.isLoading = isLoading
        self.isEnabled = isEnabled
    }
}

public struct LoadableButton<Label: View>: View {
    private let isLoading: Bool
    private let isEnabled: Bool
    private let action: () -> Void
    @ViewBuilder private let label: () -> Label

    private var isTapEnabled: Bool {
        isEnabled && !isLoading
    }

    public init(
        isLoading: Bool = false,
        isEnabled: Bool = true,
        action: @escaping () -> Void,
        @ViewBuilder label: @escaping () -> Label
    ) {
        self.isLoading = isLoading
        self.isEnabled = isEnabled
        self.action = action
        self.label = label
    }

    public init(
        configuration: LoadableButtonConfiguration,
        action: @escaping () -> Void,
        @ViewBuilder label: @escaping () -> Label
    ) {
        isLoading = configuration.isLoading
        isEnabled = configuration.isEnabled
        self.action = action
        self.label = label
    }

    public var body: some View {
        Button {
            guard isTapEnabled else { return }
            action()
        } label: {
            ZStack {
                label()
                    .opacity(isLoading ? 0 : 1)

                if isLoading {
                    ProgressView()
                }
            }
        }
        .disabled(!isTapEnabled)
    }
}
