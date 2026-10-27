const AVATAR_COLORS = [
  "#e6007a",
  "#56f39a",
  "#ffd37f",
  "#7f9dff",
  "#ff8f6b",
  "#b98cff",
];

/** The color an initial avatar takes for `seed`, the same everywhere the host draws it. */
export function avatarColor(seed: string): string {
  const index =
    [...seed].reduce((sum, char) => sum + char.charCodeAt(0), 0) %
    AVATAR_COLORS.length;
  return AVATAR_COLORS[index]!;
}
