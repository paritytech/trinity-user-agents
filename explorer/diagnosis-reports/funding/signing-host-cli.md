## Truapi Signing Host CLI Funding Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Funding/request_in_delivered` | ✅ |  |
| `Funding/request_out_released` | ✅ |  |
| `Funding/request_failed` | ✅ |  |
| `Funding/request_dismissed` | ✅ |  |
| `Funding/status_subscribe_unknown` | ✅ |  |
| `Funding/provider_assigned` | ❌ | never served the session fs_MJUVVYVsvw |
| `Funding/provider_present_frame` | ❌ | presentFrame: {"error":{"tag":"Domain","value":{"tag":"V1","value":{"tag":"NotFound"}}}} |
| `Funding/provider_reports_forward_only` | ❌ | report AwaitingPayment refused: {"tag":"Domain","value":{"tag":"V1","value":{"tag":"NotFound"}}} |
| `Funding/provider_credits_through_top_up` | ❌ | report Crediting refused: {"tag":"Domain","value":{"tag":"V1","value":{"tag":"NotFound"}}} |
| `Funding/provider_out_released` | ❌ | never served the session fs_l1mO21l9sd |
| `Funding/provider_cancel` | ❌ | never served the session fs_SA1fpC9dbd |
| `Funding/provider_resumes_after_restart` | ❌ | the first run left no session to resume |
