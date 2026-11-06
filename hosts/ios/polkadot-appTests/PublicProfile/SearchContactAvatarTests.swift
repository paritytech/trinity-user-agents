@testable import PolkadotUI
import Testing
@testable import polkadot_app

@MainActor
struct SearchContactAvatarTests {
    @Test
    func anAvatarLoadsOverTheContactsLetter() throws {
        let placeholder = AvatarViewModel.colored(text: "A", colorSeed: "alice.01")
        let icon = StaticImageViewModel(image: nil)
        let configuration = SearchContactListConfiguration(
            userName: "alice.01",
            avatarViewModel: placeholder,
            icon: icon
        )

        let view = try #require(configuration.makeContentView() as? SearchContactListView)

        #expect(view.avatarView.viewModel == placeholder)
        #expect((view.avatarView.icon as? StaticImageViewModel) === icon)
    }

    @Test
    func aReusedRowClearsThePreviousContactsAvatar() throws {
        let configuration = SearchContactListConfiguration(
            userName: "alice.01",
            avatarViewModel: .colored(text: "A", colorSeed: "alice.01"),
            icon: StaticImageViewModel(image: nil)
        )
        let view = try #require(configuration.makeContentView() as? SearchContactListView)
        let placeholder = AvatarViewModel.colored(text: "B", colorSeed: "bob.02")

        view.configuration = SearchContactListConfiguration(
            userName: "bob.02",
            avatarViewModel: placeholder
        )

        #expect(view.avatarView.viewModel == placeholder)
        #expect(view.avatarView.icon == nil)
    }
}
