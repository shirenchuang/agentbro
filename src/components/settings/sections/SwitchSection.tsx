import { useState } from 'react'
import { SettingSection } from '../SettingSection'
import { SwitchAppTabs } from './switch/SwitchAppTabs'
import { SwitchProviderList } from './switch/SwitchProviderList'
import { SwitchImportPanel } from './switch/SwitchImportPanel'
import { SwitchHealthPanel } from './switch/SwitchHealthPanel'
import './SwitchSection.css'

type SwitchView = 'providers' | 'health' | 'import'

const NAV_ITEMS: { id: SwitchView; label: string }[] = [
  { id: 'providers', label: '供应商' },
  { id: 'health', label: '健康检测' },
  { id: 'import', label: '数据导入' },
]

export function SwitchSection() {
  const [view, setView] = useState<SwitchView>('providers')

  return (
    <SettingSection
      title="API 供应商"
      description="管理 API 供应商、健康检测与 CC Switch 数据迁移。"
    >
      <div className="switch-section">
        <div className="switch-tabs">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`switch-tab${view === item.id ? ' switch-tab--active' : ''}`}
              onClick={() => setView(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>

        {view === 'providers' && (
          <>
            <SwitchAppTabs />
            <SwitchProviderList />
          </>
        )}

        {view === 'health' && (
          <>
            <SwitchAppTabs />
            <SwitchHealthPanel />
          </>
        )}

        {view === 'import' && <SwitchImportPanel />}
      </div>
    </SettingSection>
  )
}
