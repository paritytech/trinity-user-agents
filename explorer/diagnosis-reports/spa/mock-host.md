## TrUAPI Mock Host Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Account/connection_status_subscribe` | ❌ | Subscription interrupted |
| `Account/get_account` | ✅ |  |
| `Account/get_account_alias` | ❌ | TrUAPI request p:7 (wire 2, 8) timed out after 120000ms |
| `Account/create_account_proof` | ✅ |  |
| `Account/get_legacy_accounts` | ✅ |  |
| `Account/get_user_id` | ❌ | getUserId failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "No primary username for this session" } } } } } |
| `Account/request_login` | ✅ |  |
| `Account/sign_vrf` | ✅ |  |
| `Account/register_ring_vrf_key` | ❌ | timed out after 10s |
| `Account/list_ring_vrf_keys` | ✅ |  |
| `Account/ring_vrf_sign` | ❌ | ringVrfSign failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "KeyNotRegistered" } } } } |
| `Chain/follow_head_subscribe` | ❌ | timed out after 10s |
| `Chain/get_head_header` | ❌ | timed out after 10s |
| `Chain/get_head_body` | ❌ | timed out after 10s |
| `Chain/get_head_storage` | ❌ | timed out after 10s |
| `Chain/call_head` | ❌ | timed out after 10s |
| `Chain/unpin_head` | ❌ | timed out after 10s |
| `Chain/continue_head` | ❌ | timed out after 10s |
| `Chain/stop_head_operation` | ❌ | timed out after 10s |
| `Chain/get_spec_genesis_hash` | ❌ | timed out after 10s |
| `Chain/get_spec_chain_name` | ❌ | timed out after 10s |
| `Chain/get_spec_properties` | ❌ | timed out after 10s |
| `Chain/broadcast_transaction` | ❌ | timed out after 10s |
| `Chain/stop_transaction` | ❌ | timed out after 10s |
| `Chain/get_chain_info` | ✅ |  |
| `Coin Payment/create_purse` | ❌ | createPurse failed: { "error": { "tag": "Unsupported" } } |
| `Coin Payment/query_purse` | ❌ | queryPurse failed: { "error": { "tag": "Unsupported" } } |
| `Coin Payment/rebalance_purse` | ❌ | Subscription interrupted |
| `Coin Payment/delete_purse` | ❌ | Subscription interrupted |
| `Coin Payment/create_receivable` | ❌ | createReceivable failed: { "error": { "tag": "Unsupported" } } |
| `Coin Payment/create_cheque` | ❌ | createCheque failed: { "error": { "tag": "Unsupported" } } |
| `Coin Payment/deposit` | ❌ | Subscription interrupted |
| `Coin Payment/refund` | ❌ | Subscription interrupted |
| `Coin Payment/listen_for_payment` | ❌ | Subscription interrupted |
| `Entropy/derive` | ✅ |  |
| `Local Storage/read` | ✅ |  |
| `Local Storage/write` | ✅ |  |
| `Local Storage/clear` | ✅ |  |
| `Locale/subscribe` | ❌ | Subscription interrupted |
| `Notifications/send_push_notification` | ✅ |  |
| `Notifications/cancel_push_notification` | ✅ |  |
| `Payment/balance_subscribe` | ❌ | Subscription interrupted |
| `Payment/top_up` | ❌ | topUp failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } } } } } |
| `Payment/request` | ❌ | topUp failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } } } } } |
| `Payment/status_subscribe` | ❌ | topUp failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "Payments are not supported in dot.li" } } } } } |
| `Permissions/request_device_permission` | ✅ |  |
| `Permissions/request_remote_permission` | ✅ |  |
| `Preimage/lookup_subscribe` | ❌ | submit failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "signing host: Bulletin allowance allocation is native-only" } } } } } |
| `Preimage/submit` | ❌ | submit failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "Unknown", "value": { "reason": "signing host: Bulletin allowance allocation is native-only" } } } } } |
| `Resource Allocation/request` | ❌ | statement-store or bulletin allowance was not allocated: { "outcomes": [ "NotAvailable", "NotAvailable", "NotAvailable", "Allocated" ] } |
| `Signing/create_transaction` | ❌ | timed out after 190s |
| `Signing/create_transaction_with_legacy_account` | ❌ | timed out after 190s |
| `Signing/sign_raw_with_legacy_account` | ❌ | no legacy accounts available |
| `Signing/sign_payload_with_legacy_account` | ✅ |  |
| `Signing/sign_raw` | ✅ |  |
| `Signing/sign_payload` | ✅ |  |
| `Signing/sign_raw_unwatermarked_deprecated` | ✅ |  |
| `Signing/sign_raw_unwatermarked_deprecated_with_legacy_account` | ❌ | no legacy accounts available |
| `Statement Store/subscribe` | ❌ | createProofAuthorized failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "UnableToSign" } } } } |
| `Statement Store/create_proof` | ✅ |  |
| `Statement Store/submit` | ❌ | createProofAuthorized failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "UnableToSign" } } } } |
| `Statement Store/create_proof_authorized` | ❌ | createProof failed: { "error": { "tag": "Domain", "value": { "tag": "V1", "value": { "tag": "UnableToSign" } } } } |
| `System/handshake` | ✅ |  |
| `System/feature_supported` | ✅ |  |
| `System/navigate_to` | ✅ |  |
| `System/host_info` | ✅ |  |
| `System/get_product_context` | ✅ |  |
| `Theme/subscribe` | ❌ | Subscription interrupted |
