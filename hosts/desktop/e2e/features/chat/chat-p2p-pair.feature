@chat @allure.label.parentSuite:chat @allure.label.suite:Chat_P2P_Pair @allure.label.feature:Chat_P2P_Pair
Feature: P2P Chat between two Electron clients (PB-217)

  Two real Electron instances (Alice and Bob), each signed in with a
  distinct bot identity, chat peer-to-peer via the statement store.
  Each scenario resets local chat state before running so there is no
  cross-test coupling.

  Background:
    Given Alice and Bob are both authenticated
    And no chat session exists between Alice and Bob

  # Covers TC-7.2.1 (send request) + TC-7.2.2 (accept request); linked to 7.2.1 as primary.
  #
  # @skip: the send works now — #873 was smoldot-light 2.0.2 never terminating a statement
  # subscription, so the pre-submit expiry-floor query never settled; fixed upstream in
  # smoldot#3371 and shipped as @parity/truapi-provider 0.2.1. What still fails is the
  # receive: Alice's draft dismisses, then Bob's chat list stays empty and no new-requests
  # item ever appears within 90s. Un-skip when Bob receives the request.
  @allure.id:14823 @skip
  Scenario: TC-7.2.1 Alice sends a chat request by username, Bob accepts
    When Alice opens the chat as a tab
    And Alice opens the contact search
    And Alice types Bob's username into the contact search
    And Alice selects Bob from the search results
    And Alice types "hey Bob" into the welcome message field
    And Alice clicks "Send Request"
    When Bob opens the chat as a tab
    And Bob opens the new requests list
    And Bob accepts the incoming request
    Then a chat session with Alice appears in Bob's chat sidebar

  # Covers TC-7.2.3 (exchange messages) + TC-7.4.2 (react); linked to 7.2.3 as primary.
  @allure.id:14825 @skip
  Scenario: TC-7.2.3 Alice and Bob exchange messages and reactions
    # Establish the session from scratch.
    When Alice opens the chat as a tab
    And Alice opens the contact search
    And Alice types Bob's username into the contact search
    And Alice selects Bob from the search results
    And Alice types "let's chat" into the welcome message field
    And Alice clicks "Send Request"
    And Bob opens the chat as a tab
    And Bob opens the new requests list
    And Bob accepts the incoming request
    And a chat session with Alice appears in Bob's chat sidebar
    And Bob selects the chat session with Alice
    # Bob's first outgoing message is what promotes Alice's "Waiting…" outgoing
    # request into an actual session on her side — accept signal alone doesn't
    # consistently do it (known app bug).
    And Bob sends the message "hello from Bob"
    And Alice selects the chat session with Bob
    Then the message "hello from Bob" is visible in Alice's chat
    When Alice sends the message "hi Bob"
    Then the message "hi Bob" is visible in Bob's chat
    # Reactions — Alice reacts to Bob's message; both sides should see the pill.
    When Alice reacts with "👍" to the message "hello from Bob"
    Then the reaction "👍" is visible in Alice's chat
    And the reaction "👍" is visible in Bob's chat

  # ── Module 7.3 — Requests (request-stage only; no full bidirectional session) ──
  # These need only Alice's request to post on-chain (one direction); the
  # outgoing half (7.3.2/7.3.3/7.3.4) is asserted entirely on Alice's side, the
  # incoming half (7.3.1/7.3.5) once the request lands in Bob's sidebar.
  #
  # @skip: all five are blocked by the paseo-next identity backend — a freshly
  # attested pair identity (e.g. Bob) does not become searchable in contact
  # search ("contact-result-item" for the peer never appears), so Alice can't
  # select Bob to send the request. Same outage as chat-p2p.feature TC-7.1.x.
  # Bodies are authored and ready; un-skip and run twice-green once the identity
  # backend reliably indexes newly-attested peers.

  @allure.id:14836 @skip
  Scenario: TC-7.3.5 Decline an incoming request with confirmation
    When Alice opens the chat as a tab
    And Alice opens the contact search
    And Alice types Bob's username into the contact search
    And Alice selects Bob from the search results
    And Alice types "let me in Bob" into the welcome message field
    And Alice clicks "Send Request"
    When Bob opens the chat as a tab
    And Bob opens the new requests list
    And Bob declines the incoming request
    Then Bob confirms the decline
    And no incoming requests remain in Bob's chat sidebar

  @allure.id:14832 @skip
  Scenario: TC-7.3.1 New Requests counter and list
    When Alice opens the chat as a tab
    And Alice opens the contact search
    And Alice types Bob's username into the contact search
    And Alice selects Bob from the search results
    And Alice types "counter test" into the welcome message field
    And Alice clicks "Send Request"
    When Bob opens the chat as a tab
    Then Bob's chat sidebar shows 1 new request
    When Bob opens the new requests list
    Then an incoming request is shown in Bob's requests list

  @allure.id:14833 @skip
  Scenario: TC-7.3.2 Active/outgoing requests listed in sidebar
    When Alice opens the chat as a tab
    And Alice opens the contact search
    And Alice types Bob's username into the contact search
    And Alice selects Bob from the search results
    And Alice types "outgoing test" into the welcome message field
    And Alice clicks "Send Request"
    Then Alice's outgoing request to Bob is listed in her chat sidebar

  @allure.id:14834 @skip
  Scenario: TC-7.3.3 Open an outgoing pending request room
    When Alice opens the chat as a tab
    And Alice opens the contact search
    And Alice types Bob's username into the contact search
    And Alice selects Bob from the search results
    And Alice types "pending room" into the welcome message field
    And Alice clicks "Send Request"
    And Alice opens her outgoing request to Bob
    Then Alice sees the outgoing pending request room

  @allure.id:14835 @skip
  Scenario: TC-7.3.4 Cancel/remove an outgoing request
    When Alice opens the chat as a tab
    And Alice opens the contact search
    And Alice types Bob's username into the contact search
    And Alice selects Bob from the search results
    And Alice types "cancel me" into the welcome message field
    And Alice clicks "Send Request"
    And Alice opens her outgoing request to Bob
    And Alice removes her outgoing request
    Then Alice has no outgoing requests in her chat sidebar
