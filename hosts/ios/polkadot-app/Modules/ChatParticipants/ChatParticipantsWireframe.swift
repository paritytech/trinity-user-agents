import Foundation

@MainActor
final class ChatParticipantsWireframe: ChatParticipantsWireframeProtocol {
    private weak var productRoom: ChatProductRoomPresenter?

    init(productRoom: ChatProductRoomPresenter) {
        self.productRoom = productRoom
    }

    func showParticipant(_ participant: ChatRoomParticipant, from _: ChatParticipantsViewProtocol?) {
        productRoom?.didSelectParticipant(participant)
    }
}
