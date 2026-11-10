import * as v from 'valibot';

// The network's TLD as the protocol registry reports it: a single DNS label with
// a leading dot (`.dot`, `.paseo`). Validated because it crosses the chain trust
// boundary and is interpolated into a regexp by `isSafeDotNsIdentifier`.
export const networkTldSchema = v.pipe(v.string(), v.regex(/^\.[a-z0-9][a-z0-9-]{0,62}$/));
