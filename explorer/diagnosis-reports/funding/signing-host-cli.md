## Truapi Signing Host CLI Funding Diagnosis

| Method | Status | Details |
| --- | --- | --- |
| `Funding/request_in_delivered` | ❌ | {"tag":"HostFailure","value":{"reason":"funding records were stored by a newer app (format 4)"}} |
| `Funding/request_out_released` | ❌ | {"tag":"HostFailure","value":{"reason":"funding records were stored by a newer app (format 4)"}} |
| `Funding/request_failed` | ❌ | {"tag":"HostFailure","value":{"reason":"funding records were stored by a newer app (format 4)"}} |
| `Funding/request_dismissed` | ❌ | Error: host transcript never recorded the dismissed overlay |
| `Funding/status_subscribe_unknown` | ❌ | {"reason":{"tag":"HostFailure","value":{"reason":"funding records were stored by a newer app (format 4)"}},"name":"SubscriptionError"} |
