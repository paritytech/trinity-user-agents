# Regression Test Plan 900 — Case Reference

Source: test plan 900, "Regression", on the project's Allure TestOps instance (`ALLURE_ENDPOINT` / `ALLURE_PROJECT_ID`).
**270 cases, 14 modules.** This reference + the `e2e/features` suite are the source of truth for
automation status (the former `regression-automation-plan.md` has been removed).

Automation status as of 2026-09-16 — **189 of 270 linked** to automation via `@allure.id`:

- **`+` (174)** — linked and active (each verified green at least twice locally).
- **`s` (15)** — linked but `@skip`: body/page-objects/testids are ready, held by an external blocker
  (mostly the paseo-next chat identity backend; a couple of sub-second-timing onboarding cases).
- **`-` (81)** — not automated: genuinely HARD / manual-by-decision, app feature not yet implemented,
  or infra-blocked. See the relevant `.feature` file comments for the per-case reason.

> **TC-2.1.4 / TC-2.1.5 were de-automated** when e2e sign-in moved off the signing bot: both drove the
> in-app signing-bot panel, which no longer exists. Their TestOps titles still name the bot — rename
> them there, not here; this file mirrors TestOps.

Format: `<allureCaseId> | <auto?> | <TC-id> <name>`, where `<auto?>` ∈ {`+`, `s`, `-`} per above. The
numeric `allureCaseId` is what an e2e scenario tags with `@allure.id:<id>` to auto-link to this case
on result upload.

```
14675 | + | TC-1.1.1 App launches and shows the onboarding screen on first run
14676 | + | TC-1.1.2 QR code renders on the onboarding screen
14677 | - | TC-1.1.3 Returning user with a valid session skips onboarding straight to dashboard
14678 | - | TC-1.1.4 Right-clicking the system panel / title bar does not crash the app
14679 | + | TC-1.1.5 Toolbar buttons align correctly when entering/leaving fullscreen
14680 | - | TC-1.1.6 Window focus tinting on macOS
14681 | + | TC-1.2.1 Default environment is Paseo Next V2 on a fresh start
14682 | + | TC-1.2.2 Switching environment reloads the app and re-pairs on the new network
14683 | - | TC-1.2.3 Network segment buttons are disabled while pairing is in progress
14684 | - | TC-1.2.4 Pairing error shows retry and allows re-authentication
14685 | - | TC-1.2.5 "Logging in" info toast appears when pairing reaches pending
14686 | + | TC-1.3.1 Skip onboarding goes to dashboard without a session
14687 | + | TC-1.3.2 After skipping, the user can still open Settings and log in
14688 | + | TC-1.3.3 Dashboard button does not get stuck in an endless loading state
14689 | + | TC-2.1.1 Sign in on Paseo Next V2 via signing bot reaches the dashboard
14690 | s | TC-2.1.2 Sign in on Previewnet environment
14691 | - | TC-2.1.3 Sign in on Paseo Next (v1) via signing bot
14692 | - | TC-2.1.4 Signing-bot health indicator reflects reachability
14693 | - | TC-2.1.5 Connect button is disabled until QR payload is available
14694 | + | TC-2.2.1 SSO root & identity keys fetched from the PApp on sign-in
14695 | - | TC-2.2.2 SSO mapper distinguishes explicit vs implicit on create_transaction
14696 | + | TC-2.2.3 Full username is displayed and updates in the profile popover
14697 | + | TC-2.2.4 Session persists across app restart
14698 | + | TC-2.3.1 Logout clears the session and returns to onboarding
14699 | + | TC-2.3.2 Logout while a product iframe has focus still closes the popover
14700 | - | TC-2.4.1 User button reflects connected / reconnecting / offline states
14701 | - | TC-2.4.2 Reconnection happens automatically, not only after tapping the profile
14702 | + | TC-2.4.3 No-connection state shown before sign-in
14703 | - | TC-2.5.1 Legacy SSO sessions blob is migrated without a DataView error
14704 | - | TC-2.5.2 Pre-Paseo-Next-V2 settings shape resets cleanly to default environment
14705 | + | TC-3.1.1 Dashboard renders after onboarding
14706 | + | TC-3.1.2 Empty dashboard shows empty state with Add Widget CTA
14707 | + | TC-3.1.3 Home button returns to dashboard page 1
14708 | + | TC-3.1.4 Pagination dots switch dashboard pages
14709 | + | TC-3.1.5 Dashboard scrolls/paginates on small window width
14710 | - | TC-3.1.6 Cross-page widget drag creates a provisional trailing page
14711 | + | TC-3.2.1 Open Add Widget modal from toolbar
14712 | + | TC-3.2.2 Add Widget modal lists only widget-capable products not already in a folder
14713 | + | TC-3.2.3 Search filters the Add Widget product list
14714 | + | TC-3.2.4 Add a product as a widget updates the dashboard layout immediately
14715 | + | TC-3.2.5 Add to Favorites for a widget product places a 1x1 icon
14716 | + | TC-3.2.6 Already-on-dashboard widget shows Open instead of Add
14717 | + | TC-3.2.7 Adding to favorites that are already present is a no-op
14718 | + | TC-3.3.1 Product widget body loads its webview (Electron)
14719 | + | TC-3.3.2 Reload a product widget from its card topbar
14720 | + | TC-3.3.3 Open product widget fullscreen from its card
14721 | + | TC-3.3.4 1x1 product shortcut card opens product fullscreen
14722 | + | TC-3.3.5 Resize a widget via its card menu
14723 | + | TC-3.3.6 Clean up dashboard via widget menu
14724 | + | TC-3.3.7 Remove a widget via its card menu
14725 | - | TC-3.3.8 Widget card menu only affects the active page
14726 | + | TC-3.4.1 Favorites folder appears once a favorite is added
14727 | + | TC-3.4.2 Remove folder keeps the page open
14728 | + | TC-3.5.1 Widget product shows "Add to Dashboard" in actions menu
14729 | + | TC-3.5.2 Widget-less product shows "Add to Favorites" toggle in actions menu
14730 | + | TC-3.5.3 Remove from Favorites via actions menu
14731 | + | TC-3.5.4 Open product Settings from actions menu
14732 | + | TC-4.1.1 Open a product by typing a dotNS domain
14733 | + | TC-4.1.2 Invalid / special-character input does not crash the app
14734 | + | TC-4.1.3 Address bar suggestions: recents, favorites, and results
14735 | + | TC-4.1.4 Keyboard navigation and ghost-suffix autocomplete
14736 | + | TC-4.1.5 Escape closes the address suggestions
14737 | + | TC-4.1.6 Click outside closes the address bar suggestions
14738 | - | TC-4.1.7 In-progress input is preserved across app switch (Cmd+Tab)
14739 | - | TC-4.1.8 Product name centering in address bar
14740 | + | TC-4.1.9 Address bar shows loading progress while a product resolves
14741 | + | TC-4.1.10 Trailing/leading-slash normalization in typed paths
14742 | + | TC-4.2.1 Tab bar hidden in single-active-tab (one-tab) mode
14743 | + | TC-4.2.2 Open multiple tabs and switch between them
14744 | - | TC-4.2.3 Active tab and titles/icons are centre-aligned
14745 | - | TC-4.2.4 Close button positioned to the left of the tab
14746 | - | TC-4.2.5 Tab separators hide around active/hovered tabs
14747 | + | TC-4.2.6 Reorder tabs by drag without font change
14748 | + | TC-4.2.7 Tab hover card shows RAM usage and pin glyph
14749 | + | TC-4.2.8 Closing the last tab returns to the dashboard
14750 | - | TC-4.2.9 Tab strip wheel-scroll and fade mask with many tabs
14751 | + | TC-4.3.1 Cycle tabs with Ctrl+Tab / Ctrl+Shift+Tab
14752 | + | TC-4.3.2 Jump to tab by number (Cmd/Ctrl+1..9)
14753 | + | TC-4.3.3 Cmd/Ctrl+T opens a new tab and focuses address bar
14754 | + | TC-4.3.4 Cmd/Ctrl+R reloads the current tab's webview
14755 | + | TC-4.3.5 Cmd/Ctrl+W closes the current tab
14756 | + | TC-4.4.1 Back/Forward buttons enable per tab history
14757 | + | TC-4.4.2 Back/Forward via keyboard and menu accelerators
14758 | + | TC-4.4.3 Buttons are correctly toolbar-positioned and left-aligned
14759 | + | TC-4.5.1 In-product pushState on a visible tab updates host route
14760 | + | TC-4.5.2 Backgrounded tab pushState does not hijack the host route
14761 | + | TC-4.5.3 Cross-product polkadot:// link does not replace source tab
14762 | + | TC-4.5.4 Re-opening a backgrounded product root via address bar does not freeze
14763 | + | TC-4.5.5 External http(s) link opens in the system browser
14764 | + | TC-4.6.1 Open find bar with Cmd/Ctrl+F on a product
14765 | + | TC-4.6.2 Search matches, navigate next/previous, and counter
14766 | + | TC-4.6.3 No-results state in find bar
14767 | + | TC-4.6.4 Close find bar
14768 | + | TC-4.6.5 Find shortcut is inert off product routes
14769 | + | TC-4.7.1 Zoom in/out/reset on a product page
14770 | + | TC-4.7.2 Zoom indicator buttons adjust zoom
14771 | + | TC-4.7.3 Zoom indicator does not flash on tab switch / reload
14772 | + | TC-4.7.4 Zoom shortcut is inert off product routes
14773 | + | TC-4.8.1 New tab page shows wordmark, address bar, pinned and recent apps
14774 | + | TC-4.8.2 Clear recents with undo on new tab
14775 | + | TC-4.9.1 Switch theme (System / Light / Dark)
14776 | + | TC-5.1.1 Open the product actions menu from the address bar
14777 | + | TC-5.1.2 Open product settings from the actions menu
14778 | + | TC-5.1.3 Reload an open product
14779 | + | TC-5.1.4 Product SPA content remains visible over time
14780 | + | TC-5.2.1 Render a product widget on the dashboard
14781 | + | TC-5.2.2 Widget shows "not found" when its executable cannot be resolved
14782 | + | TC-5.2.3 Reload a product widget via its reload control
14783 | + | TC-5.3.1 Clear product cache from settings
14784 | + | TC-5.3.2 Forget an app removes it and its local storage
14785 | + | TC-5.3.3 Cancel Forget app dialog leaves product intact
14786 | + | TC-5.4.1 Account APIs work in the product sandbox
14787 | + | TC-5.4.2 Sign raw message in the product sandbox
14788 | s | TC-5.4.3 Signing still works after reloading the product
14789 | + | TC-5.4.4 Resource allocation request modal (allowances)
14790 | - | TC-5.5.1 Rate-limit toast includes product name and limiter type
14791 | + | TC-6.1.1 Allow Always grants a device permission persistently
14792 | + | TC-6.1.2 Allow Once grants for the current request only
14793 | + | TC-6.1.3 Deny a device permission blocks and persists denial
14794 | + | TC-6.1.4 Dismiss permission dialog defaults to denied
14795 | + | TC-6.1.5 Notification permission is prompted only once
14796 | + | TC-6.2.1 Allow Always for an external (Remote) origin
14797 | + | TC-6.2.2 Allow Once for an external origin does not persist
14798 | + | TC-6.2.3 Deny external origin shows blocked toast
14799 | - | TC-6.2.4 Queued remote permission prompts are shown one at a time
14800 | + | TC-6.3.1 View an app's requested permissions
14801 | + | TC-6.3.2 Change an app's permission status via the per-app entity page
14802 | + | TC-6.3.3 Reset an app permission to default
14803 | - | TC-6.3.4 Manage External Request (web domains) per app
14804 | - | TC-6.3.5 Bulk-change ExternalRequest status applies to all patterns
14805 | + | TC-6.4.1 View permissions list grouped by category
14806 | + | TC-6.4.2 Permission detail lists apps using it
14807 | + | TC-6.4.3 Change a permission's status per app from the permission detail page
14808 | - | TC-6.4.4 Clipboard permission denied → allowed transition takes effect
14809 | + | TC-6.5.1 Approve alias access with Allow Always
14810 | + | TC-6.5.2 Deny and Allow Once for alias access
14811 | + | TC-6.5.3 Manage alias contexts from app settings
14812 | + | TC-6.6.1 Allowance update modal on existing allocation
14813 | + | TC-6.7.1 Permission modal text is legible in dark theme
14814 | + | TC-7.1.1 Open contact search from fullscreen chat
14815 | s | TC-7.1.2 Search finds a peer by username
14816 | + | TC-7.1.3 Search with no matches shows empty state
14817 | - | TC-7.1.4 Direct-connect by SS58 address
14818 | + | TC-7.1.5 Clear search query
14819 | s | TC-7.1.6 Select a peer and view the welcome step
14820 | s | TC-7.1.7 Back navigation from welcome step
14821 | - | TC-7.1.8 Already-connected peer cannot be re-selected
14822 | - | TC-7.1.9 Copy own chat address from sidebar header
14823 | + | TC-7.2.1 Send a chat request with a welcome message
14824 | - | TC-7.2.2 Accept an incoming request and create a session
14825 | s | TC-7.2.3 Exchange text messages between two clients
14826 | + | TC-7.2.4 Send via Enter; newline via Shift+Enter
14827 | + | TC-7.2.5 Send button disabled for empty/whitespace input
14828 | - | TC-7.2.6 Outgoing message status icons progress
14829 | + | TC-7.2.7 Message grouping and date separators
14830 | - | TC-7.2.8 Auto-scroll to latest and mark-as-read
14831 | + | TC-7.2.9 Empty session shows "no messages" placeholder
14832 | s | TC-7.3.1 New Requests counter and list
14833 | s | TC-7.3.2 Active/outgoing requests are listed in the sidebar
14834 | s | TC-7.3.3 Open an outgoing pending request room
14835 | s | TC-7.3.4 Cancel/remove an outgoing request
14836 | s | TC-7.3.5 Decline an incoming request with confirmation
14837 | + | TC-7.4.1 Open the message context menu
14838 | - | TC-7.4.2 React to a message via quick-reaction row
14839 | + | TC-7.4.3 Open full emoji picker and react
14840 | - | TC-7.4.4 Toggle a reaction off
14841 | - | TC-7.4.5 Reaction pill tooltip and counts
14842 | + | TC-7.4.6 Reply to a message
14843 | + | TC-7.4.7 Edit an own text message
14844 | + | TC-7.4.8 Edit not offered for incoming or non-text messages
14845 | - | TC-7.4.9 View edit history
14846 | + | TC-7.4.10 Copy message text
14847 | - | TC-7.4.11 Forward message action (design pending)
14848 | + | TC-7.4.12 Delete the conversation from the room header menu
14849 | - | TC-7.5.1 Attach and preview an image before send
14850 | - | TC-7.5.2 Attach a non-image file shows file-card preview
14851 | - | TC-7.5.3 Send a message with attachments
14852 | - | TC-7.5.4 Remove a selected attachment before send
14853 | - | TC-7.5.5 Oversized file is rejected
14854 | + | TC-7.6.1 Empty chat list state
14855 | + | TC-7.6.2 Room list sorted by latest activity
14856 | + | TC-7.6.3 Chat widget on the dashboard lists sessions and supports fullscreen
14857 | + | TC-7.6.4 QuickChat popover from the top bar
14858 | - | TC-7.6.5 QuickChat: open a session, send, and back-navigate
14859 | + | TC-7.6.6 QuickChat "View more" opens chat tab
14860 | + | TC-7.7.1 Add CoinFlip widget and chat via QuickChat
14861 | - | TC-7.7.2 Product "Proceed in Chat" opens the chat; the worker declares the room
14862 | + | TC-8.1.1 Sign raw message succeeds in product sandbox
14863 | s | TC-8.1.2 Sign raw message still works after product reload
14864 | + | TC-8.1.3 Rejecting a raw-message request surfaces rejected toast
14865 | + | TC-8.2.1 Sign payload review screen shows account, network, fee and call title
14866 | + | TC-8.2.2 More details expands arguments and call data
14867 | s | TC-8.2.3 Custom-chain signing shows raw-call warning and hides fee
14868 | s | TC-8.2.4 Batch call behaviour hint shown for utility.batch variants
14869 | s | TC-8.2.5 Continue to Sign disabled until People chain connected
14870 | + | TC-8.2.6 Double-click on Continue to Sign does not start two signings
14871 | + | TC-8.3.1 createTransaction request opens modal and returns signed tx
14872 | - | TC-8.3.2 createTransaction is denied for a mismatched signer identifier
14873 | - | TC-8.3.3 ReviveApi_call signing path produces a valid signature
14874 | - | TC-8.4.1 Sign payload with legacy account when address derives from product account
14875 | - | TC-8.4.2 Legacy-account signing rejected when address cannot be derived
14876 | - | TC-8.4.3 Legacy accounts list reports one account (accounts API)
14877 | - | TC-8.5.1 Signing times out instead of hanging when remote signer is unresponsive
14878 | - | TC-8.5.2 Signing failure toast opens error details dialog
14884 | - | TC-8.5.3 Sequential signing requests are queued one modal at a time
14885 | - | TC-8.5.4 Host Playground signing flows behave consistently
14886 | - | TC-8.5.5 Polkadot App step animation and lifetime
14887 | + | TC-9.1.1 Environment selector lists Previewnet, Paseo Next and Paseo Next V2
14888 | + | TC-9.1.2 Switching environment while signed in prompts logout confirmation
14889 | + | TC-9.1.3 Cancelling the environment-change dialog keeps current environment
14890 | + | TC-9.1.4 Selecting the same environment is a no-op
14891 | - | TC-9.1.5 Environment change with no active session reloads immediately
14892 | - | TC-9.2.1 Add a custom chain via a valid WSS endpoint
14893 | + | TC-9.2.2 Reject an invalid (non-ws) endpoint URL
14894 | - | TC-9.2.3 Reject a duplicate of a built-in or already-added custom chain
14895 | + | TC-9.2.4 Connection failure surfaces an error toast
14896 | - | TC-9.2.5 Remove a custom chain
14897 | + | TC-10.1.1 Settings sidebar shows Preferences, Privacy and Development groups
14898 | - | TC-10.1.2 Settings header is centered and matches title spec
14899 | + | TC-10.1.3 Settings back/forward navigation preserves selected sub-page
14900 | + | TC-10.2.1 Theme settings expose System, Light, and Dark cards
14901 | + | TC-10.2.2 Switching to Dark theme applies and persists
14902 | + | TC-10.2.3 Switching to System theme follows OS appearance
14903 | + | TC-10.2.4 Permission request modal text is readable in dark theme
14904 | + | TC-10.3.1 User popover shows display name and connected status banner
14905 | + | TC-10.3.2 Full username is shown in the profile popup
14906 | - | TC-10.3.3 Popover reflects reconnecting and offline states
14907 | + | TC-10.3.4 Log out from the user popover ends the session
14908 | + | TC-10.3.5 Alias permission request screen renders correctly
14909 | + | TC-10.4.1 Product receives theme-change notifications via Host API
14910 | + | TC-11.1.1 Immediate push notification fires and is titled with the product name
14911 | - | TC-11.1.2 Clicking a notification focuses the window and deep-links into the product
14912 | + | TC-11.1.3 Cancel a scheduled notification
14913 | - | TC-11.1.4 Scheduled (future) push notification fires at the scheduled time
14914 | + | TC-11.1.5 Schedule limit reached returns ScheduleLimitReached
14915 | + | TC-11.2.1 Rate-limited push shows a toast with product name and limiter type
14916 | - | TC-11.2.2 Forgetting a product cancels its outstanding notifications
14917 | - | TC-11.2.3 Boot reconcile cancels notifications for products removed while closed
14918 | - | TC-12.1.1 "Check for Updates" from the app menu opens the update modal
14919 | - | TC-12.1.2 "Check for updates" button works on the Onboarding screen
14920 | - | TC-12.1.3 "Check for Updates" button in Settings → Development → Update channel
14921 | - | TC-12.1.4 Up-to-date result
14922 | - | TC-12.1.5 Manual check error is surfaced
14923 | - | TC-12.2.1 Download progress and install-now flow in the modal
14924 | - | TC-12.2.2 "Not now" in ready-to-install keeps the app running
14925 | - | TC-12.3.1 Background "update available" toast appears with version
14926 | - | TC-12.3.2 Dismissing the toast suppresses it for that version
14927 | - | TC-12.3.3 Toast install and install-failed states
14928 | - | TC-12.4.1 Switching update channel re-checks against the new feed
14929 | - | TC-12.5.1 App does not get stuck in infinite loading after an update
14930 | - | TC-12.5.2 Toolbar layout correct after auto-update on macOS fullscreen
14931 | + | TC-13.1.1 Pin a product for offline use
14932 | + | TC-13.1.2 Cancel the Enable offline dialog
14933 | + | TC-13.2.1 Remove offline access from a pinned product
14934 | - | TC-13.3.1 Pinned product loads while offline
14935 | - | TC-13.3.2 Non-pinned product fails gracefully offline
14936 | - | TC-13.4.1 Update offline version when a newer version is available
14937 | + | TC-13.5.1 Forgetting a pinned product clears its offline cache and local storage
14938 | + | TC-14.1.1 localStorage is isolated between products
14939 | + | TC-14.1.2 forgetAndReset wipes the product's sandbox storage
14940 | + | TC-14.1.3 Storage APIs remain accessible within a product's own partition
14941 | + | TC-14.2.1 Node.js context is fully isolated from product code
14942 | + | TC-14.2.2 Network requests from products are blocked outside the allowlist
14943 | + | TC-14.2.3 Device permissions (camera/microphone) are blocked for products
14944 | + | TC-14.2.4 CSP meta tag is present with restrictive directives
14945 | + | TC-14.3.1 External navigation and dangerous protocols are blocked
14946 | + | TC-14.3.2 IPC store access is restricted to authorized keys
14947 | + | TC-14.3.3 Archive injection with malformed domains is rejected
14948 | + | TC-14.3.4 Deep-link / URL injection cannot crash or inject script
14949 | + | TC-14.4.1 Unresponsive product overlay appears when the main thread wedges
```
