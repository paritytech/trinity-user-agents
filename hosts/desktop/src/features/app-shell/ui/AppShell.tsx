import { type PropsWithChildren, memo } from 'react';

import { useSlot } from '@/shared/di';
import { FaviconProvider } from '../context/FaviconContext';
import { persistentSlot } from '../di';

import { Favicon } from './Favicon';
import { Header } from './Header';

export const AppShell = memo(({ children }: PropsWithChildren) => {
  const persistent = useSlot(persistentSlot);

  return (
    <FaviconProvider>
      <div className="flex h-full w-full flex-col animate-in fade-in" style={{ appRegion: 'no-drag' }}>
        <Favicon />
        <Header />
        <div className="flex min-h-0 w-full grow overflow-hidden">
          <main className="relative h-full min-h-0 min-w-0 grow overflow-hidden bg-bg-surface-nested">{children}</main>
        </div>
        {persistent}
      </div>
    </FaviconProvider>
  );
});
