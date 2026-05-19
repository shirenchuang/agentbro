import { useEffect, useState } from 'react'
import { useSwitchStore } from '../../../../stores/switchStore'

const STEPS = ['检测', '预览', '导入', '完成'] as const

export function SwitchImportPanel() {
  const { ccSwitchDetected, importPreview, importResult, importing, error, detectCcSwitch, previewImport, runImport, clearAllData } = useSwitchStore()
  const [confirmClear, setConfirmClear] = useState(false)
  const [clearing, setClearing] = useState(false)
  const [clearSuccess, setClearSuccess] = useState(false)

  useEffect(() => {
    detectCcSwitch()
  }, [])

  useEffect(() => {
    if (ccSwitchDetected) {
      previewImport()
    }
  }, [ccSwitchDetected])

  const currentStep = importResult ? 3 : importing ? 2 : importPreview ? 1 : 0

  const handleClear = async () => {
    setClearing(true)
    await clearAllData()
    setClearing(false)
    setConfirmClear(false)
    setClearSuccess(true)
    setTimeout(() => setClearSuccess(false), 3000)
  }

  const stepIndicator = (
    <div className="switch-import-steps">
      {STEPS.map((label, i) => (
        <div key={label} className="switch-import-step-wrapper">
          {i > 0 && <span className="switch-import-step__arrow">›</span>}
          <span
            className={`switch-import-step${i === currentStep ? ' switch-import-step--active' : ''}${i < currentStep ? ' switch-import-step--done' : ''}`}
          >
            <span className="switch-import-step__dot">
              {i < currentStep ? '✓' : i + 1}
            </span>
            {label}
          </span>
        </div>
      ))}
    </div>
  )

  return (
    <div className="switch-import-panel">
      <h3>从 CC Switch 导入</h3>
      {stepIndicator}

      {error && <div className="switch-error">{error}</div>}

      {ccSwitchDetected === null && (
        <div className="switch-loading">正在检测 CC Switch 安装...</div>
      )}

      {ccSwitchDetected === false && (
        <div>
          <p>
            未找到 CC Switch 数据库（<code>~/.cc-switch/cc-switch.db</code>）。
            请确认 CC Switch 已启动过至少一次。
          </p>
        </div>
      )}

      {ccSwitchDetected && !importResult && (
        <>
          <p>检测到 CC Switch，可将已有配置导入 AgentBro。</p>

          {importPreview && (
            <div className="switch-import-panel__preview">
              <h4>待导入数据：</h4>
              <ul>
                <li>{importPreview.providers} 个供应商</li>
                {importPreview.provider_endpoints > 0 && (
                  <li>{importPreview.provider_endpoints} 个供应商端点</li>
                )}
                <li>{importPreview.mcp_servers} 个 MCP 服务</li>
                <li>{importPreview.prompts} 个 Prompt</li>
                <li>{importPreview.skills} 个技能</li>
              </ul>
            </div>
          )}

          <button
            type="button"
            className="switch-btn switch-btn--primary"
            disabled={importing}
            onClick={runImport}
          >
            {importing ? '导入中...' : '开始导入'}
          </button>
        </>
      )}

      {importResult && (
        <div className="switch-import-panel__result">
          <h3>导入完成</h3>
          <ul>
            <li>已导入 {importResult.providers_imported} 个供应商</li>
            {importResult.provider_endpoints_imported > 0 && (
              <li>已导入 {importResult.provider_endpoints_imported} 个端点</li>
            )}
            <li>已导入 {importResult.mcp_servers_imported} 个 MCP 服务</li>
            <li>已导入 {importResult.prompts_imported} 个 Prompt</li>
            <li>已导入 {importResult.skills_imported} 个技能</li>
          </ul>
        </div>
      )}

      {clearSuccess && (
        <div className="switch-success">所有数据已清除</div>
      )}

      {/* 危险区域 */}
      <div className="switch-danger-zone">
        <h4>调试工具</h4>
        <p>清除 Switch 模块所有数据（供应商、Prompt、用量日志、设置等），方便调试时重置。</p>
        <button
          type="button"
          className="switch-btn switch-btn--danger"
          onClick={() => setConfirmClear(true)}
        >
          一键清除所有数据
        </button>
      </div>

      {confirmClear && (
        <div className="switch-confirm-overlay">
          <div className="switch-confirm-dialog">
            <p>确定清除所有 Switch 数据？此操作不可撤销，将删除所有供应商、Prompt、用量日志和设置。</p>
            <div className="switch-confirm-dialog__actions">
              <button type="button" className="switch-btn" onClick={() => setConfirmClear(false)}>
                取消
              </button>
              <button
                type="button"
                className="switch-btn switch-btn--danger"
                disabled={clearing}
                onClick={handleClear}
              >
                {clearing ? '清除中...' : '确认清除'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
