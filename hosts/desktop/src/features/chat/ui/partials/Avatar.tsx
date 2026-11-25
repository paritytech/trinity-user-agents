import { Avatar as TrUiAvatar } from '@novasamatech/tr-ui';

import { type AvatarSize, avatarSizeMap, getAvatarTone } from '../helpers/avatar';

type AvatarProps = {
  name: string;
  size?: AvatarSize;
};

export const Avatar = ({ name, size = 'medium' }: AvatarProps) => (
  <TrUiAvatar label={name} alt={name} tone={getAvatarTone(name)} size={avatarSizeMap[size]} />
);
