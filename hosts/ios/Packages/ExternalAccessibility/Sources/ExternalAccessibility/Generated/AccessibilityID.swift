// swiftlint:disable all
// Generated from accessibility-ids.yaml in polkadot-app-external-accessibility.
// Do NOT edit manually — edit the registry in that repo and re-run:
//   python3 generate.py

public enum AccessibilityID {
    public enum Settings: String, AccessibilityIdentifying, CaseIterable {
        /// Backup entry row on the Settings main screen
        case backupButton = "settings_backup_button"
        /// "View Secret Recovery Phrase" button on the Backup screen
        case viewRecoveryPhraseButton = "settings_view_recovery_phrase_button"
        /// "Show recovery phrase" button on the recovery warning sheet
        case showRecoveryPhraseButton = "settings_show_recovery_phrase_button"
        /// "Tap to reveal" overlay on the secret phrase screen
        case tapToRevealButton = "settings_tap_to_reveal_button"
        /// "Copy to clipboard" button on the secret phrase screen
        case copyRecoveryPhraseButton = "settings_copy_recovery_phrase_button"
    }

    public enum Backup: String, AccessibilityIdentifying, CaseIterable {
        /// Backup to iCloud button on the Backup screen
        case toIcloudButton = "backup_to_icloud_button"
    }

    public enum Pairing: String, AccessibilityIdentifying, CaseIterable {
        /// Title of the "Link a new device?" pairing modal
        case title = "pairing_title"
        /// Confirm (Link) button on the pairing modal
        case confirmButton = "pairing_confirm_button"
        /// Reject (Cancel) button on the pairing modal
        case rejectButton = "pairing_reject_button"
    }

    public enum Signing: String, AccessibilityIdentifying, CaseIterable {
        /// Signing request title ("<host> requires signature")
        case requestTitle = "signing_request_title"
        /// Approve (Sign) button on the signing confirmation sheet
        case approveButton = "signing_approve_button"
        /// Reject button on the signing confirmation sheet
        case rejectButton = "signing_reject_button"
    }

    public enum Chats: String, AccessibilityIdentifying, CaseIterable {
        /// Title of the Chats main screen
        case title = "chats_title"
        /// New-chat "+" bar button on the Chats main screen
        case newChatButton = "chats_new_chat_button"
        /// Username search field on the new-chat sheet
        case newChatUsernameInput = "chats_new_chat_username_input"
        /// Cancel button on the new-chat sheet
        case newChatCancelButton = "chats_new_chat_cancel_button"
        /// Invite message input on the pending-request chat screen
        case inviteMessageInput = "chats_invite_message_input"
        /// Invite send button on the pending-request chat screen
        case inviteSendButton = "chats_invite_send_button"
        /// "Invite {username} to chat" header on the pending-request chat screen
        case inviteHeader = "chats_invite_header"
        /// Contact row in new-chat search results
        case searchResultRow = "chats_search_result_row"
        /// Chat row in the chats main list
        case chatRow = "chats_chat_row"
        /// "New requests" entry on the chats main list
        case newRequestsItem = "chats_new_requests_item"
        /// Request row on the incoming chat requests screen
        case requestRow = "chats_request_row"
    }

    public enum Chat: String, AccessibilityIdentifying, CaseIterable {
        /// Message input in the chat feed
        case messageInput = "chat_message_input"
        /// Send button in the chat feed input bar
        case sendButton = "chat_send_button"
        /// Send-funds ($) button in the chat feed input bar
        case sendFundsButton = "chat_send_funds_button"
        /// Attach (+) button in the chat feed input bar
        case attachButton = "chat_attach_button"
        /// Send button on the attachment preview sheet
        case attachmentSendButton = "chat_attachment_send_button"
        /// Reaction emoji picker panel shown on message long-press
        case reactionEmojiPanel = "chat_reaction_emoji_panel"
        /// Accept button on the incoming chat request banner
        case acceptRequestButton = "chat_accept_request_button"
        /// Voice-call bar button on the chat screen
        case voiceCallButton = "chat_voice_call_button"
        /// Transfer status label on a transfer message (Detecting/Confirmed)
        case transferStatusLabel = "chat_transfer_status_label"
        /// Container bubble of a transfer message
        case transferMessageBubble = "chat_transfer_message_bubble"
    }

    public enum SendPayment: String, AccessibilityIdentifying, CaseIterable {
        /// Recipient username input on the Send Payment screen
        case recipientInput = "send_payment_recipient_input"
        /// Recipient row in Send Payment search results
        case searchResultRow = "send_payment_search_result_row"
    }

    public enum TransferAmount: String, AccessibilityIdentifying, CaseIterable {
        /// "Available now" label on the Enter Amount screen
        case availableBalanceLabel = "transfer_amount_available_balance_label"
        /// Available balance value on the Enter Amount screen
        case availableBalanceValue = "transfer_amount_available_balance_value"
        /// Currency symbol label next to the amount input
        case currencyLabel = "transfer_amount_currency_label"
        /// Amount input field on the Enter Amount screen
        case input = "transfer_amount_input"
        /// Submit button (ENTER AMOUNT / Send $X) on the Enter Amount screen
        case submitButton = "transfer_amount_submit_button"
    }

    public enum IncomingCall: String, AccessibilityIdentifying, CaseIterable {
        /// Ringing status label on the incoming call screen
        case statusLabel = "incoming_call_status_label"
        /// Answer button on the incoming call screen
        case answerButton = "incoming_call_answer_button"
        /// Decline button on the incoming call screen
        case declineButton = "incoming_call_decline_button"
    }

    public enum InCall: String, AccessibilityIdentifying, CaseIterable {
        /// Status/duration label on the active call screen
        case statusLabel = "in_call_status_label"
        /// End-call (hang-up) button on the active call screen
        case endButton = "in_call_end_button"
        /// Audio route (speaker) button on the active call screen
        case volumeButton = "in_call_volume_button"
        /// Mute button on the active call screen
        case muteButton = "in_call_mute_button"
    }

    public enum Game: String, AccessibilityIdentifying, CaseIterable {
        /// Polkadot Peer (Weekly Game) row in the chat list
        case polkadotPeerChat = "game_polkadot_peer_chat"
        /// Game registration card in the Polkadot Peer chat
        case registerMessage = "game_register_message"
        /// Register button on the game registration card
        case registerButton = "game_register_button"
        /// Pinned title of the Weekly Game chat screen
        case weeklyTitle = "game_weekly_title"
        /// "Why gestures" welcome card in the Polkadot Peer chat
        case gestureCard = "game_gesture_card"
        /// "Play once to get your Membership" welcome card in the Polkadot Peer chat
        case membershipCard = "game_membership_card"
    }

    public enum Onboarding: String, AccessibilityIdentifying, CaseIterable {
        /// "Recover here" link on the claim username screen
        case recoverHere = "onboarding_recover_here"
        /// Continue button on the onboarding theme picker
        case themeContinueButton = "onboarding_theme_continue_button"
    }

    public enum Username: String, AccessibilityIdentifying, CaseIterable {
        /// Username text field on the claim username screen
        case input = "username_input"
        /// Username submit button on the claim username flow
        case submitButton = "username_submit_button"
    }

    public enum RecoveryPhrase: String, AccessibilityIdentifying, CaseIterable {
        /// Recovery phrase input on the account recovery screen
        case input = "recovery_phrase_input"
        /// Recovery phrase submit button on the account recovery screen
        case submitButton = "recovery_phrase_submit_button"
    }

    public enum Wallet: String, AccessibilityIdentifying, CaseIterable {
        /// Username display on the identity plastic card
        case usernameDisplay = "wallet_username_display"
        /// CASH card on the Pocket screen
        case cashCard = "wallet_cash_card"
        /// Collapsed balance on the CASH card
        case cashCardBalance = "wallet_cash_card_balance"
        /// Total balance text on the CASH card
        case totalBalance = "wallet_total_balance"
        /// Top up pill on the expanded CASH card
        case addFundsButton = "wallet_add_funds_button"
        /// Withdraw pill on the expanded CASH card
        case withdrawButton = "wallet_withdraw_button"
        /// Send pill on the expanded CASH card
        case sendPaymentButton = "wallet_send_payment_button"
        /// "Make all vouchers ready" button in the coinage breakdown
        case makeVouchersReadyButton = "wallet_make_vouchers_ready_button"
        /// Coinage Balance section header
        case coinageHeader = "wallet_coinage_header"
        /// Total Balance row label in the coinage breakdown
        case coinageTotalBalanceLabel = "wallet_coinage_total_balance_label"
        /// Total Balance row value in the coinage breakdown
        case coinageTotalBalanceValue = "wallet_coinage_total_balance_value"
        /// Spendable Balance row label in the coinage breakdown
        case coinageSpendableBalanceLabel = "wallet_coinage_spendable_balance_label"
        /// Spendable Balance row value in the coinage breakdown
        case coinageSpendableBalanceValue = "wallet_coinage_spendable_balance_value"
        /// Pending Balance row label in the coinage breakdown
        case coinagePendingBalanceLabel = "wallet_coinage_pending_balance_label"
        /// Pending Balance row value in the coinage breakdown
        case coinagePendingBalanceValue = "wallet_coinage_pending_balance_value"
        /// Close bar button on the expanded wallet detail overlay
        case detailCloseButton = "wallet_detail_close_button"
        /// Close button on the wallet backup notification card
        case backupNotificationCloseButton = "wallet_backup_notification_close_button"
        /// "Yes, I'm done" confirm button on the backup cancel sheet
        case backupDoneConfirmButton = "wallet_backup_done_confirm_button"
    }

    public enum Tab: String, AccessibilityIdentifying, CaseIterable {
        /// Chat tab bar item
        case chat = "tab_chat"
        /// Wallet (Pocket) tab bar item
        case wallet = "tab_wallet"
        /// Settings tab bar item
        case settings = "tab_settings"
    }
}
