import SwiftUI

public enum ChatMessageMediaButtonStyle: Hashable {
    case play
    case download
    case retry
    case loading(cancelable: Bool = false)
}

public enum ChatMessageMediaButtonSize: Hashable {
    case large
    case compact
}

public struct ChatMessageMediaButton: View {
    let style: ChatMessageMediaButtonStyle
    let size: ChatMessageMediaButtonSize
    let action: () -> Void

    var frameSize: CGFloat {
        switch size {
        case .large: 80
        case .compact: 40
        }
    }

    var imageSize: CGFloat {
        switch size {
        case .large: 40
        case .compact: 20
        }
    }

    public init(
        style: ChatMessageMediaButtonStyle,
        size: ChatMessageMediaButtonSize = .large,
        action: @escaping () -> Void = {}
    ) {
        self.style = style
        self.size = size
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            buttonContent
                .foregroundStyle(Color.fgStaticWhite)
                .frame(width: frameSize, height: frameSize)
                .background {
                    Circle()
                        .foregroundStyle(Color.bgSurfaceOverlay)
                }
        }
    }

    private var imageResource: ImageResource? {
        switch style {
        case .play: .play
        case .download: .arrowDown
        case .retry: .rotateCw
        case let .loading(cancelable): cancelable ? .xIcon : nil
        }
    }

    @ViewBuilder
    private var innerImage: some View {
        if let imageResource {
            Image(imageResource)
                .renderingMode(.template)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .frame(width: imageSize, height: imageSize)
        }
    }

    @ViewBuilder
    private var buttonContent: some View {
        switch style {
        case .play,
             .download,
             .retry:
            innerImage

        case .loading:
            ZStack {
                LoadingSpinner()
                innerImage
            }
        }
    }
}

#Preview("Play") {
    ZStack {
        ChatMessageMediaButton(style: .play)
    }
}

#Preview("Download") {
    ZStack {
        ChatMessageMediaButton(style: .download)
    }
}

#Preview("Retry") {
    ZStack {
        ChatMessageMediaButton(style: .retry)
    }
}

#Preview("Loading") {
    ZStack {
        ChatMessageMediaButton(style: .loading(cancelable: true))
    }
}
