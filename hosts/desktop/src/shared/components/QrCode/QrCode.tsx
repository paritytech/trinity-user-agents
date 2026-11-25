import QRCode from 'qrcode';
import { memo, useEffect, useState } from 'react';

type Props = {
  value: string;
  size: number;
  /** Drives the module/background colours; defaults to light. */
  theme?: 'light' | 'dark';
  className?: string;
};

const PALETTE = {
  light: { dark: '#000000ff', light: '#ffffff00' },
  dark: { dark: '#ffffffff', light: '#00000000' },
};

/**
 * Renders a string as a QR code.
 *
 * Encoding is async and the value can change while a render is in flight, so the
 * effect drops a result whose request has been superseded — otherwise a slow encode
 * of the previous value would paint over the current one.
 */
export const QrCode = memo(({ value, size, theme = 'light', className }: Props) => {
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    QRCode.toDataURL(value, { width: size, margin: 1, color: PALETTE[theme] })
      .then(url => {
        if (active) setDataUrl(url);
      })
      .catch((error: unknown) => {
        console.error('[qr] failed to encode', error);
        if (active) setDataUrl(null);
      });

    return () => {
      active = false;
    };
  }, [value, size, theme]);

  // Hold the box before the first encode lands so the surrounding layout does not
  // jump when it does.
  if (!dataUrl) return <div className={className} style={{ width: size, height: size }} />;

  return <img className={className} src={dataUrl} width={size} height={size} alt="" />;
});
