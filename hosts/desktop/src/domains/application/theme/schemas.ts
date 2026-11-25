import * as v from 'valibot';

/**
 * The persisted theme settings, validated on read.
 *
 * The stored row is a trust boundary: an older build, a device-sync write, or a user
 * with the devtools open can leave anything in it. An unrecognised value reads as
 * absent so the caller falls back to the default rather than handing an unknown
 * palette name to the UI kit.
 */
export const themePreferenceSchema = v.picklist(['light', 'dark', 'system']);

export const themeNameSchema = v.picklist(['berlin', 'tokyo', 'lisbon', 'malta']);
