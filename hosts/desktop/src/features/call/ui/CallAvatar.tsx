import { getAvatarLetter, getAvatarPalette } from '@/shared/utils';

type CallAvatarProps = { name: string; sizePx?: number };

export const CallAvatar = ({ name, sizePx = 80 }: CallAvatarProps) => {
  const letter = getAvatarLetter(name);
  const palette = getAvatarPalette(name);

  return (
    <div
      className="flex items-center justify-center rounded-full font-semibold select-none"
      style={{ width: sizePx, height: sizePx, fontSize: sizePx * 0.47, backgroundColor: palette.bg, color: palette.fg }}
    >
      {letter}
    </div>
  );
};
