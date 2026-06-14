import { useEffect, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'

export function SettingsPageV2() {
  const state = useSkillStoreV2()
  const settings = state.settings
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!settings) state.loadOverview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  if (!settings) {
    return <div className="sm2__empty">加载设置中…</div>
  }

  const update = async (patch: Parameters<typeof state.updateSettings>[0]) => {
    setBusy(true)
    try {
      await state.updateSettings(patch)
    } finally {
      setBusy(false)
    }
  }

  const chooseCenter = async () => {
    const dir = await open({ directory: true, multiple: false })
    if (typeof dir === 'string') await update({ centerPath: dir })
  }

  return (
    <div className="sm2">
      <div className="sm2__header">
        <h2 className="sm2__title">设置</h2>
      </div>
      <div className="sm2__main">
        <div className="sm2__issue">
          <h4 style={{ margin: '0 0 8px' }}>中心库路径</h4>
          <div style={{ display: 'flex', gap: 8 }}>
            <input className="sm2__search" value={settings.centerPath} readOnly />
            <button className="sm2__btn" onClick={chooseCenter} disabled={busy}>选择</button>
          </div>
          <p style={{ fontSize: 11, color: 'var(--text-secondary)', marginTop: 6 }}>
            默认 ~/.agentbro/skills。修改后下次刷新生效。
          </p>
        </div>

        <div className="sm2__issue">
          <h4 style={{ margin: '0 0 8px' }}>默认分发方式</h4>
          <select
            className="sm2__select"
            value={settings.defaultDistributeMode}
            onChange={(e) => update({ defaultDistributeMode: e.target.value as 'link' | 'copy' })}
          >
            <option value="link">link（软链接）</option>
            <option value="copy">copy（复制）</option>
          </select>
        </div>

        <div className="sm2__issue">
          <h4 style={{ margin: '0 0 8px' }}>link 失败策略</h4>
          <select
            className="sm2__select"
            value={settings.linkFailPolicy}
            onChange={(e) => update({ linkFailPolicy: e.target.value as 'ask' | 'copy' })}
          >
            <option value="ask">询问（默认阻止）</option>
            <option value="copy">自动改用 copy</option>
          </select>
        </div>

        <div className="sm2__issue">
          <h4 style={{ margin: '0 0 8px' }}>扫描行为</h4>
          <label className="sm2__checkbox-row">
            <input
              type="checkbox"
              checked={settings.startupScan}
              onChange={(e) => update({ startupScan: e.target.checked })}
            />
            启动时扫描中心库与 Agent
          </label>
          <label className="sm2__checkbox-row">
            <input
              type="checkbox"
              checked={settings.showUnmanaged}
              onChange={(e) => update({ showUnmanaged: e.target.checked })}
            />
            显示未管理 Skills
          </label>
        </div>

        <div className="sm2__issue">
          <h4 style={{ margin: '0 0 8px' }}>SQLite / JSON 快照</h4>
          <div className="sm2__detail-meta">
            <div>SQLite：{settings.sqlitePath}</div>
            <div>快照：{settings.centerPath}/agentbro-skills.snapshot.json</div>
          </div>
          <div className="sm2__btn-row">
            <button className="sm2__btn" onClick={() => skillApiV2.exportSnapshot()}>导出/刷新 JSON 快照</button>
            <button className="sm2__btn" onClick={() => skillApiV2.openPath(settings.sqlitePath)}>打开 SQLite</button>
          </div>
          <p style={{ fontSize: 11, color: 'var(--text-secondary)', marginTop: 6 }}>
            SQLite 是主存储；JSON 仅作为备份、快照与人工排查用途。
          </p>
        </div>
      </div>
    </div>
  )
}
