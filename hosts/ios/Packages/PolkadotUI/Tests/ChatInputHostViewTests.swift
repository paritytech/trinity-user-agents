import Testing
import UIKit
@testable import PolkadotUI

/// The chat's input slot cuts to a new slot, unless the new slot asks to crossfade from the one before.
@MainActor
struct ChatInputHostViewTests {
    private struct Slot: ChatInputViewConfigurationProtocol {
        let id: Int
        var fades = false

        var activateOnAppear: Bool {
            false
        }

        func makeContentView(for _: ChatInputHandling?) -> UIView {
            UIView()
        }

        func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
            (configuration as? Slot)?.id == id
        }

        func crossfades(from _: any ChatInputViewConfigurationProtocol) -> Bool {
            fades
        }
    }

    private final class Screen {
        let container = UIView(frame: CGRect(x: 0, y: 0, width: 390, height: 800))
        let keyboard = UILayoutGuide()
        let host = ChatInputHostView()

        init() {
            container.addLayoutGuide(keyboard)
            container.addSubview(host)
        }

        @discardableResult
        func bind(_ slot: Slot) -> UIView? {
            host.bind(configuration: slot, inputHandler: nil, keyboardGuide: keyboard)
            return host.contentView
        }
    }

    @Test("A slot that does not crossfade replaces the one before at once")
    func aSlotCutsToTheNext() {
        let screen = Screen()
        let first = screen.bind(Slot(id: 1))

        let second = screen.bind(Slot(id: 2))

        #expect(first?.superview == nil)
        #expect(second?.superview === screen.host)
        #expect(second?.alpha == 1)
    }

    @Test("A slot that crossfades fades in over the one before, which fades out and takes no touches")
    func aSlotCrossfadesFromTheOneBefore() {
        let screen = Screen()
        let first = screen.bind(Slot(id: 1))

        let second = screen.bind(Slot(id: 2, fades: true))

        #expect(screen.host.contentView === second)
        #expect(first?.superview === screen.host)
        #expect(first?.isUserInteractionEnabled == false)
        #expect(first?.alpha == 0)
        #expect(second?.alpha == 1)
    }

    @Test("Binding the same slot again keeps it, with no crossfade")
    func theSameSlotIsKept() {
        let screen = Screen()
        let first = screen.bind(Slot(id: 1, fades: true))

        let again = screen.bind(Slot(id: 1, fades: true))

        #expect(again === first)
        #expect(screen.host.subviews.filter { $0 === first }.count == 1)
    }

    @Test("A slot bound while another fades out drops the one still fading")
    func aNewSlotMidFadeDropsTheFadingOne() {
        let screen = Screen()
        let first = screen.bind(Slot(id: 1))
        let second = screen.bind(Slot(id: 2, fades: true))

        let third = screen.bind(Slot(id: 3, fades: true))

        #expect(first?.superview == nil)
        #expect(second?.superview === screen.host)
        #expect(third?.superview === screen.host)
        #expect(screen.host.contentView === third)
    }

    @Test("Clearing the slot mid-fade removes both slots")
    func clearingMidFadeRemovesBoth() {
        let screen = Screen()
        let first = screen.bind(Slot(id: 1))
        let second = screen.bind(Slot(id: 2, fades: true))

        screen.host.clear()

        #expect(first?.superview == nil)
        #expect(second?.superview == nil)
        #expect(screen.host.contentView == nil)
    }
}
