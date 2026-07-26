import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { skillApiV2 } from '../../services/skillApiV2'
import { skillModeLabel } from './skillLabels'

export function SettingsPageV2() {
  const { t } = useTranslation()
  const state = useSkillStoreV2()
  const settings = state.settings
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)

  useEffect(() => {
    if (!settings) state.loadOverview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  if (!settings) {
    return <div className="sm2__empty">加载设置中…</div>
  }

  const update = async (patch: Parameters<typeof state.updateSettings>[0]) => {
    setBusy(true)
    setNotice(null)
    try {
      await state.updateSettings(patch)
    } finally {
      setBusy(false)
    }
  }

  const exportSnapshot = async () => {
    setBusy(true)
    setNotice(null)
    try {
      const path = await skillApiV2.exportSnapshot()
      setNotice(path ? `JSON 快照已刷新：${path}` : 'JSON 快照已刷新')
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const revealSqlite = async () => {
    setBusy(true)
    setNotice(null)
    try {
      await skillApiV2.revealPath(settings.sqlitePath)
      setNotice('已在 Finder 中定位 SQLite')
    } catch (e) {
      state.setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2">
      <div className="sm2__header">
        <h2 className="sm2__title">设置</h2>
      </div>
      <div className="sm2__main">
        {state.error && <div className="sm2__error">{state.error}</div>}
        {notice && <div className="sm2__notice sm2__notice--ok">{notice}</div>}

        <div className="sm2__issue">
          <h4 className="sm2__settings-label">默认分发方式</h4>
          <select
            className="sm2__select"
            value={settings.defaultDistributeMode}
            onChange={(e) => update({ defaultDistributeMode: e.target.value as 'link' | 'copy' })}
          >
            <option value="link">{skillModeLabel(t, 'link')}</option>
            <option value="copy">{skillModeLabel(t, 'copy')}</option>
          </select>
        </div>

        <div className="sm2__issue">
          <h4 className="sm2__settings-label">link 失败策略</h4>
          <select
            className="sm2__select"
            value={settings.linkFailPolicy}
            onChange={(e) => update({ linkFailPolicy: e.target.value as 'ask' | 'copy' })}
          >
            <option value="ask">询问（默认阻止）</option>
            <option value="copy">自动改用 {skillModeLabel(t, 'copy')}</option>
          </select>
        </div>

        <div className="sm2__issue">
          <h4 className="sm2__settings-label">扫描行为</h4>
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
          <h4 className="sm2__settings-label">{t('skills.packAutoSyncTitle', { defaultValue: '技能包同步' })}</h4>
          <label className="sm2__checkbox-row">
            <input
              type="checkbox"
              checked={settings.autoSyncSkillPacks !== false}
              onChange={(e) => update({ autoSyncSkillPacks: e.target.checked })}
            />
            {t('skills.packAutoSyncLabel', { defaultValue: '技能包更新后自动同步到已应用的 Agent' })}
          </label>
          <p className="sm2__settings-help">
            {t('skills.packAutoSyncHelp', { defaultValue: '关闭后，技能包页面会显示有变更未同步，并提供手动同步按钮。' })}
          </p>
        </div>

        <div className="sm2__issue">
          <h4 className="sm2__settings-label">SQLite / JSON 快照</h4>
          <div className="sm2__detail-meta">
            <div>SQLite：{settings.sqlitePath}</div>
            <div>快照：{settings.centerPath}/agentbro-skills.snapshot.json</div>
          </div>
          <div className="sm2__btn-row">
            <button className="sm2__btn" onClick={exportSnapshot} disabled={busy}>导出/刷新 JSON 快照</button>
            <button className="sm2__btn" onClick={revealSqlite} disabled={busy}>在 Finder 中显示 SQLite</button>
          </div>
          <p className="sm2__settings-help">
            SQLite 是主存储；JSON 仅作为备份、快照与人工排查用途。
          </p>
        </div>
      </div>
    </div>
  )
}
