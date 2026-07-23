import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { skillApiV2 } from '../../services/skillApiV2'
import type { AdoptPreview, SkillPackSummary } from '../../services/skillApiV2'
import { PreviewDialog } from './PreviewDialog'
import { skillErrorMessage } from './skillLabels'

export function AdoptDialog({
  preview,
  packs,
  onClose,
  onDone,
}: {
  preview: AdoptPreview
  packs: SkillPackSummary[]
  onClose: () => void
  onDone: () => void | Promise<void>
}) {
  const { t } = useTranslation()
  const editablePacks = useMemo(
    () => packs
      .filter((pack) => pack.id && pack.id !== 'default')
      .sort((left, right) => left.name.localeCompare(right.name)),
    [packs],
  )
  const [option, setOption] = useState(() => preferredAdoptOption(preview.options))
  const [renamedId, setRenamedId] = useState('')
  const [addToPack, setAddToPack] = useState(false)
  const [packMode, setPackMode] = useState<'existing' | 'new'>(() => editablePacks.length > 0 ? 'existing' : 'new')
  const [packId, setPackId] = useState(() => editablePacks[0]?.id ?? '')
  const [newPackName, setNewPackName] = useState('')
  const [adoptedSkillId, setAdoptedSkillId] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const selectedOption = preview.options.find((o) => o.value === option)
  const optionCopy = selectedOption ? adoptOptionCopy(selectedOption) : null
  const effectiveRenamedId = renamedId.trim() || `${preview.inferredSkillId}-import`
  const packChoiceInvalid = addToPack && option !== 'skip' && (
    packMode === 'existing' ? !packId : !newPackName.trim()
  )

  useEffect(() => {
    setPackId((current) => editablePacks.some((pack) => pack.id === current) ? current : editablePacks[0]?.id ?? '')
    if (editablePacks.length === 0) setPackMode('new')
  }, [editablePacks])

  const execute = async () => {
    setBusy(true)
    setError(null)
    try {
      const skillId = adoptedSkillId || await skillApiV2.executeAdopt(
        preview.agentId,
        preview.unmanagedId,
        option,
        option === 'rename' ? renamedId : null,
      )
      if (option !== 'skip' && !adoptedSkillId) setAdoptedSkillId(skillId)
      if (addToPack && option !== 'skip') {
        try {
          await addSkillToPack(skillId, packMode === 'existing'
            ? { kind: 'existing', packId }
            : { kind: 'new', name: newPackName.trim() })
        } catch (e) {
          setError(t('skills.adoptPack.partialError', { error: skillErrorMessage(t, e) }))
          return
        }
      }
      await onDone()
    } catch (e) {
      setError(skillErrorMessage(t, e))
    } finally {
      setBusy(false)
    }
  }

  const close = () => {
    if (adoptedSkillId) {
      void Promise.resolve(onDone())
      return
    }
    onClose()
  }

  return (
    <PreviewDialog
      title={`接管 ${preview.inferredSkillId || '未命名 Skill'}`}
      confirmLabel={adoptedSkillId && addToPack
        ? t('skills.adoptPack.retry')
        : option === 'skip'
          ? '保持未管理'
          : '确认接管'}
      modalClassName="sm2__modal--adopt"
      destructive={selectedOption?.destructive}
      busy={busy}
      disabled={packChoiceInvalid}
      onConfirm={execute}
      onCancel={close}
    >
      <div className="sm2-adopt">
        <div className="sm2-adopt__summary">
          <div>
            <span>Skill</span>
            <strong>{preview.inferredSkillId || '未命名 Skill'}</strong>
          </div>
          <div>
            <span>中心库同名</span>
            <strong>{preview.centerHasSameId ? '已存在' : '无冲突'}</strong>
          </div>
          <div className="sm2-adopt__summary-path">
            <span>Agent 路径</span>
            <code>{preview.skillPath}</code>
          </div>
        </div>

        <section className="sm2-adopt__section">
          <div className="sm2-adopt__section-head">
            <h4>选择接管方式</h4>
            <span>{preview.canQuickAdopt ? '可以直接导入中心库' : '中心库已有不同内容，需要处理冲突'}</span>
          </div>
          <div className="sm2-adopt__options" role="radiogroup" aria-label="接管方式">
            {preview.options.map((o) => {
              const copy = adoptOptionCopy(o)
              const active = option === o.value
              return (
                <button
                  key={o.value}
                  type="button"
                  className={`sm2-adopt__option${active ? ' sm2-adopt__option--active' : ''}${o.destructive ? ' sm2-adopt__option--destructive' : ''}`}
                  role="radio"
                  aria-checked={active}
                  aria-label={copy.shortLabel}
                  disabled={Boolean(adoptedSkillId)}
                  onClick={() => {
                    setOption(o.value)
                    if (o.value === 'rename' && !renamedId.trim()) setRenamedId(`${preview.inferredSkillId}-import`)
                  }}
                >
                  <span className="sm2-adopt__radio" />
                  <span className="sm2-adopt__option-main">
                    <strong>{copy.title}</strong>
                    <span>{copy.description}</span>
                  </span>
                  <em>{copy.badge}</em>
                </button>
              )
            })}
          </div>
        </section>

        {option === 'rename' && (
          <div className="sm2-adopt__rename">
            <label htmlFor="sm2-adopt-rename">新的 Skill ID</label>
            <input
              id="sm2-adopt-rename"
              value={renamedId}
              onChange={(e) => setRenamedId(e.target.value)}
              placeholder={`${preview.inferredSkillId}-import`}
              disabled={Boolean(adoptedSkillId)}
            />
            <span>将以「{effectiveRenamedId}」写入中心库，原 Agent 文件会作为副本目标被管理。</span>
          </div>
        )}

        {option !== 'skip' && (
          <section className="sm2-adopt__section sm2-adopt-pack">
            <div className="sm2-adopt__section-head">
              <h4>{t('skills.adoptPack.title')}</h4>
              <span>{t('skills.adoptPack.hint')}</span>
            </div>
            <label className="sm2-adopt-pack__toggle">
              <input
                type="checkbox"
                checked={addToPack}
                onChange={(event) => {
                  const checked = event.currentTarget.checked
                  setAddToPack(checked)
                  if (checked && editablePacks.length > 0 && !packId) {
                    setPackMode('existing')
                    setPackId(editablePacks[0].id)
                  }
                }}
              />
              <span aria-hidden="true" />
              <div>
                <strong>{t('skills.adoptPack.toggle')}</strong>
                <small>{addToPack ? t('skills.adoptPack.enabledHelp') : t('skills.adoptPack.disabledHelp')}</small>
              </div>
            </label>

            {addToPack && (
              <div className="sm2-adopt__rename sm2-adopt-pack__target">
                <div className="sm2__view-toggle sm2__view-toggle--soft" aria-label={t('skills.adoptPack.mode')}>
                  <button
                    type="button"
                    className={packMode === 'existing' ? 'active' : ''}
                    disabled={editablePacks.length === 0}
                    onClick={() => {
                      setPackMode('existing')
                      if (!packId) setPackId(editablePacks[0]?.id ?? '')
                    }}
                  >
                    {t('skills.adoptPack.existing')}
                  </button>
                  <button
                    type="button"
                    className={packMode === 'new' ? 'active' : ''}
                    onClick={() => setPackMode('new')}
                  >
                    {t('skills.adoptPack.new')}
                  </button>
                </div>

                {packMode === 'existing' && (
                  <>
                    <label htmlFor="sm2-adopt-pack-existing">{t('skills.adoptPack.targetPack')}</label>
                    <select
                      id="sm2-adopt-pack-existing"
                      value={packId}
                      onChange={(event) => setPackId(event.target.value)}
                    >
                      {editablePacks.map((pack) => (
                        <option key={pack.id} value={pack.id}>
                          {pack.name} ({pack.memberCount})
                        </option>
                      ))}
                    </select>
                    <span>{editablePacks.length > 0 ? t('skills.adoptPack.existingImpact') : t('skills.adoptPack.noPacks')}</span>
                  </>
                )}

                {packMode === 'new' && (
                  <>
                    <label htmlFor="sm2-adopt-pack-new">{t('skills.adoptPack.newPackName')}</label>
                    <input
                      id="sm2-adopt-pack-new"
                      value={newPackName}
                      onChange={(event) => setNewPackName(event.target.value)}
                      placeholder={t('skills.adoptPack.newPackPlaceholder')}
                    />
                    <span>{t('skills.adoptPack.newImpact')}</span>
                  </>
                )}
              </div>
            )}
          </section>
        )}

        {optionCopy && (
          <div className={`sm2-adopt__impact${selectedOption?.destructive ? ' sm2-adopt__impact--warn' : ''}`}>
            <strong>{selectedOption?.destructive ? '会改动 Agent 目录' : '不会删除现有文件'}</strong>
            <span>{optionCopy.impact}</span>
          </div>
        )}

        {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      </div>
    </PreviewDialog>
  )
}

type AdoptPackSelection =
  | { kind: 'existing'; packId: string }
  | { kind: 'new'; name: string }

async function addSkillToPack(skillId: string, selection: AdoptPackSelection) {
  if (selection.kind === 'new') {
    await skillApiV2.upsertPack({
      id: '',
      name: selection.name,
      description: '',
      tags: [],
      skillIds: [skillId],
    })
    return
  }

  const pack = await skillApiV2.getPackDetail(selection.packId)
  const existingIds = pack.members.map((member) => member.skillId)
  if (existingIds.includes(skillId)) return
  await skillApiV2.upsertPack({
    id: pack.id,
    name: pack.name,
    description: pack.description,
    tags: pack.tags,
    skillIds: [...existingIds, skillId],
  })
}

function adoptOptionCopy(option: { value: string; label: string; destructive: boolean }) {
  switch (option.value) {
    case 'import_keep':
      return {
        title: '导入中心库，保留 Agent 文件',
        shortLabel: '保留 Agent 文件',
        description: '把这个 Skill 纳入中心库记录，Agent 目录里的原文件保持不动。',
        badge: '保留',
        impact: '适合先接管已有文件，后续再决定是否改成链接或副本。',
      }
    case 'import_link':
      return {
        title: '导入中心库，并替换为链接',
        shortLabel: '替换为软连接',
        description: '中心库成为唯一来源，Agent 目录改为指向中心库的链接。',
        badge: '推荐',
        impact: '会删除当前 Agent 目录中的 Skill 文件夹，再创建到中心库的链接。',
      }
    case 'import_copy':
      return {
        title: '导入中心库，并替换为副本',
        shortLabel: '替换为复制',
        description: '中心库保存一份，Agent 目录重新写入一份托管副本。',
        badge: '副本',
        impact: '会删除当前 Agent 目录中的 Skill 文件夹，再从中心库复制回 Agent。',
      }
    case 'import_cleanup':
      return {
        title: '导入中心库，并清理 .agents 原目录',
        shortLabel: '清理 .agents 原目录',
        description: '先复制到中心库，成功后删除 .agents/skills 中的原 Skill，避免共享目录隐式生效。',
        badge: '推荐',
        impact: '会删除 .agents/skills 里的原 Skill 文件夹；中心库会保留可管理副本。',
      }
    case 'center_over_agent':
      return {
        title: '使用中心库版本接管',
        shortLabel: '中心库为准',
        description: '保留中心库已有 Skill，把当前 Agent 目录替换为中心库链接。',
        badge: '推荐',
        impact: '不会改动中心库；会删除当前 Agent 目录中的同名 Skill 文件夹，再创建到中心库的链接。',
      }
    case 'overwrite_center':
      return {
        title: '用当前文件覆盖中心库',
        shortLabel: '覆盖中心库',
        description: '中心库已有同名 Skill，将以这个 Agent 里的版本为准。',
        badge: '覆盖',
        impact: '会替换中心库同名 Skill 的内容，请确认当前 Agent 文件是正确版本。',
      }
    case 'rename':
      return {
        title: '改名导入中心库',
        shortLabel: '重命名导入',
        description: '保留中心库已有同名 Skill，把当前文件作为新的 Skill ID 导入。',
        badge: '改名',
        impact: '不会覆盖中心库已有 Skill，也不会删除当前 Agent 文件。',
      }
    case 'skip':
      return {
        title: '暂不接管',
        shortLabel: '暂不接管',
        description: '保持这个 Skill 为未管理状态，本次不写入中心库。',
        badge: '跳过',
        impact: '不会改动中心库或 Agent 目录；下次扫描仍可能看到它。',
      }
    default:
      return {
        title: option.label,
        shortLabel: option.label,
        description: '使用 AgentBro 后端建议的处理方式。',
        badge: option.destructive ? '会改动' : '安全',
        impact: option.destructive ? '执行前请确认该操作会修改现有文件。' : '该操作不会删除现有文件。',
      }
  }
}

function preferredAdoptOption(options: Array<{ value: string }>) {
  return options.find((option) => option.value === 'center_over_agent')?.value
    || options.find((option) => option.value === 'import_cleanup')?.value
    || options.find((option) => option.value === 'import_link')?.value
    || options[0]?.value
    || 'import_keep'
}
