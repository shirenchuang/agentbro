import { MarketPanel } from './InstallView'

export function OfficialSourcesPanel({ onInstall, onDone }: { onInstall?: (source?: string) => void; onDone: (skillId?: string) => void }) {
  return (
    <div className="sm2__official sm2__official--market-only">
      <MarketPanel onInstall={onInstall || (() => {})} onDone={onDone} />
    </div>
  )
}
