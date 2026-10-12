---
"@parity/truapi-host": patch
---

An SSO request waiting on the paired signing host skips a peer message that does not decode, with a debug log, instead of failing. A stale reply left on the session channel, such as an AutoSigning allocation without `ring_vrf_domain_entropy`, is skipped, so later backups, identity publishing and `getProductAccount` calls complete, and the idle peer-disconnect monitor keeps watching past it.
