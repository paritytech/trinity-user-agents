import Foundation
import UIKit

extension ChatProductRoomPresenter {
    /// Draws the bar items of the room: the participants button of the host, when the room has participants.
    func provideToolbar() {
        view?.didReceive(productToolbar: participantsButton().map { [$0] } ?? [])
        mark("toolbar", "participants=\(asksParticipants)")
    }
}
