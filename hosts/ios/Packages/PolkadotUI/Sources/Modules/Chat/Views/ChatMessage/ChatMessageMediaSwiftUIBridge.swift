import SwiftUI

struct ChatMessageMediaSwiftUIBridge: UIViewRepresentable {
    let configuration: ChatMessageMediaViewConfiguration

    init(configuration: ChatMessageMediaViewConfiguration) {
        self.configuration = configuration
    }

    func makeUIView(context _: Context) -> ChatMessageMediaView {
        ChatMessageMediaView(configuration: configuration)
    }

    func updateUIView(_ uiView: ChatMessageMediaView, context _: Context) {
        uiView.configuration = configuration
    }
}

extension ChatMessageMediaViewConfiguration {
    func makeSwiftUIView() -> some View {
        ChatMessageMediaSwiftUIBridge(configuration: self)
    }
}
