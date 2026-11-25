@chat @allure.label.parentSuite:chat @allure.label.suite:Chat_Seeded @allure.label.feature:Chat_Seeded
Feature: Chat — seeded session message display and actions

  UI-over-existing-data chat coverage that needs NO live peer/handshake. A chat
  session + messages are written straight into the chat domain's standalone
  Dexie DB (`p2p-chat`) via `helpers/seed-chat.ts`, so a fully populated room
  renders without the P2P attestation backend. Cases requiring live message
  delivery (reaction round-trip, send) stay on the two-client pair suite.

  Background:
    Given the user is authenticated

  @allure.id:14829
  Scenario: TC-7.2.7 Message grouping and date separators
    Given the chat conversation "Seeded Grouping" is seeded across two days
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Grouping"
    Then at least 2 date separators are shown
    And the "Today" date separator is shown

  @allure.id:14831
  Scenario: TC-7.2.9 Empty session shows "no messages" placeholder
    Given an empty chat conversation "Seeded Empty" is seeded
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Empty"
    Then the no-messages placeholder is shown

  @allure.id:14837
  Scenario: TC-7.4.1 Open the message context menu
    Given the chat conversation "Seeded Menu" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Menu"
    And the user opens the message context menu on "Hello there"
    Then the message context menu is shown

  @allure.id:14839
  # The reaction WRITE needs live delivery (session.sendMessage); only the
  # picker-opens portion is automatable without a live peer — see seed-chat note.
  Scenario: TC-7.4.3 Open the full emoji picker
    Given the chat conversation "Seeded Emoji" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Emoji"
    And the user opens the full emoji picker on "Hello there"
    Then the emoji picker is shown

  @allure.id:14842
  Scenario: TC-7.4.6 Reply to a message
    Given the chat conversation "Seeded Reply" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Reply"
    And the user replies to "Hello there"
    Then the reply composer is shown

  @allure.id:14843
  Scenario: TC-7.4.7 Edit an own text message
    Given the chat conversation "Seeded Edit" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Edit"
    And the user edits the own message "My own message"
    Then the edit composer is shown

  @allure.id:14844
  Scenario: TC-7.4.8 Edit not offered for an incoming message
    Given the chat conversation "Seeded EditGate" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded EditGate"
    And the user opens the context menu on the incoming message "Hello there"
    Then the edit action is not offered
    When the user opens the context menu on the own message "My own message"
    Then the edit action is offered

  @allure.id:14846
  Scenario: TC-7.4.10 Copy message text
    Given the chat conversation "Seeded Copy" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Copy"
    And the user copies the message "Hello there"
    Then the clipboard contains "Hello there"

  @allure.id:14848
  Scenario: TC-7.4.12 Delete the conversation from the room header menu
    Given the chat conversation "Seeded Delete" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Delete"
    And the user deletes the conversation from the header menu
    Then the conversation "Seeded Delete" is removed from the chat list

  @allure.id:14855
  Scenario: TC-7.6.2 Room list sorted by latest activity
    Given two chat conversations "Seeded Older" and "Seeded Newer" are seeded with different activity
    When the user opens the chat fullscreen view
    Then the conversation "Seeded Newer" is listed before "Seeded Older"

  @allure.id:14827
  Scenario: TC-7.2.5 Send button disabled for empty/whitespace input
    Given the chat conversation "Seeded Composer" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Composer"
    Then the chat send control is unavailable
    When the user replaces the chat composer with "   "
    Then the chat send control is unavailable
    When the user replaces the chat composer with "Hello"
    Then the chat send control is available

  @allure.id:14826
  # Composer-side only: Shift+Enter inserts a newline; Enter is bound to submit
  # (suppresses the newline). The Enter SEND delivery hits session.sendMessage,
  # which a seeded room has no keys for, so the message is not delivered — only
  # the composer behaviour (newline vs submit) is asserted here. The delivered
  # round-trip stays on the two-client chat-p2p-pair suite.
  Scenario: TC-7.2.4 Send via Enter; newline via Shift+Enter
    Given the chat conversation "Seeded Newline" is seeded with sample messages
    When the user opens the chat fullscreen view
    And the user opens the seeded conversation "Seeded Newline"
    And the user types "alpha" into the chat composer
    And the user presses "Shift+Enter" in the chat composer
    And the user types "beta" into the chat composer
    Then the chat composer contains a newline
    When the user replaces the chat composer with "single line"
    And the user presses "Enter" in the chat composer
    Then the chat composer has no newline

  @allure.id:14856
  Scenario: TC-7.6.3 Chat widget on the dashboard lists sessions and supports fullscreen
    Given the chat conversation "Seeded Widget" is seeded with sample messages
    When the user adds the chat widget to the dashboard
    Then the dashboard chat widget lists the conversation "Seeded Widget"
    When the user opens the dashboard chat widget fullscreen
    Then the chat fullscreen route is shown
