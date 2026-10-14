import Foundation

@MainActor
final class SearchContactWireframe: SearchContactWireframeProtocol {
    var onChatFound: ((ChatOpenModel) -> Void)?

    func complete(from _: SearchContactViewProtocol?, with model: ChatOpenModel) {
        onChatFound?(model)
    }
}
