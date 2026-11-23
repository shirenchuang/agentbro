import { useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useSkillStoreV2 } from '../../stores/skillStoreV2'
import { useRuntimeEnvironmentStore } from '../../stores/runtimeEnvironmentStore'
import { skillApiV2 } from '../../services/skillApiV2'
import type {
  AgentSummary,
  ConflictBlocker,
  DeleteSkillPackPreview,
  DistributionBlockerDecision,
  DistributionPreview,
  RemovePackFromAgentPreview,
  RemoveSkillFromPackPreview,
  SkillPackDetail,
  SkillPackFingerprint,
  SkillPackManifest,
  SkillPackSummary,
} from '../../services/skillApiV2'
import { PreviewDialog } from './PreviewDialog'
import { AgentIconBadge } from './AgentIconBadge'
import { SkillDetailSlider } from './SkillDetailSlider'
import { skillModeLabel, skillSourceTypeLabel } from './skillLabels'

type BuilderMode = 'create' | 'edit' | 'duplicate'
type PackRuntime = Pick<ReturnType<typeof useRuntimeEnvironmentStore.getState>, 'selectedEnvironmentId' | 'selectionRevision'>
type PackExportMode = 'definition' | 'sources' | 'migration'
type PackImport = {
  manifest: SkillPackManifest
  sourceUrl?: string
  runtime: PackRuntime
  reviewed: SkillPackFingerprint[] | null
  acceptLocalCopies: boolean
  restoreErrors: Map<string, string>
  restoredFrom: Map<string, string>
  copyDraft?: { skillId: string; destinationId: string }
}
type PackLink = {
  runtime: PackRuntime
  url: string
  loading: boolean
  error: string | null
}
type PackMemberStatus = 'missing' | 'unavailable' | 'unrecorded' | 'different' | 'match'
type PackRemoval =
  | { kind: 'delete'; preview: DeleteSkillPackPreview }
  | { kind: 'member'; preview: RemoveSkillFromPackPreview }
  | { kind: 'revoke'; preview: RemovePackFromAgentPreview }
type PackRemovalReview = PackRemoval & { runtime: PackRuntime }
type PackSyncConflictState = {
  packId: string
  runtime: PackRuntime
  targetAgents: string[]
  requestedMode: 'link' | 'copy'
  preview: DistributionPreview
}
type SyncPackOptions = {
  background?: boolean
  packName?: string
}
type PackBuilderSaveOptions = {
  forceSync?: boolean
}
type SkillPackMembershipsBySkillId = Record<string, SkillPackSummary[]>

const SHARED_SKILLS_AGENT_ID = 'agents'
const DEFAULT_SKILL_PACK_ID = 'default'
type BlockerDecision = DistributionBlockerDecision['action']

function useSkillPackMemberships(packs: SkillPackSummary[], selectedDetail: SkillPackDetail | null) {
  const [result, setResult] = useState<{
    key: string
    membershipsBySkillId: SkillPackMembershipsBySkillId
  }>({ key: '', membershipsBySkillId: {} })
  const customPacks = useMemo(
    () => packs.filter((pack) => pack.id !== DEFAULT_SKILL_PACK_ID),
    [packs],
  )
  const requestKey = useMemo(() => [
    ...customPacks.map((pack) => `${pack.id}:${pack.memberCount}:${pack.revision || ''}`),
    selectedDetail
      ? `${selectedDetail.id}:${selectedDetail.revision || ''}:${selectedDetail.members.map((member) => member.skillId).join(',')}`
      : '',
  ].join('|'), [customPacks, selectedDetail])

  useEffect(() => {
    let cancelled = false
    if (customPacks.length === 0) return

    void (async () => {
      const details = await Promise.allSettled(customPacks.map(async (pack) => ({
        pack,
        detail: selectedDetail?.id === pack.id
          ? selectedDetail
          : await skillApiV2.getPackDetail(pack.id),
      })))
      if (cancelled) return

      const next: SkillPackMembershipsBySkillId = {}
      for (const result of details) {
        if (result.status !== 'fulfilled' || !result.value.detail?.members) continue
        const { pack, detail } = result.value
        for (const member of detail.members) {
          if (member.missing) continue
          if (!next[member.skillId]) next[member.skillId] = []
          next[member.skillId].push(pack)
        }
      }
      setResult({ key: requestKey, membershipsBySkillId: next })
    })()

    return () => {
      cancelled = true
    }
  }, [customPacks, requestKey, selectedDetail])

  if (customPacks.length === 0) return { membershipsBySkillId: {}, loading: false }
  return {
    membershipsBySkillId: result.key === requestKey ? result.membershipsBySkillId : {},
    loading: result.key !== requestKey,
  }
}

function blockerKey(blocker: ConflictBlocker) {
  return `${blocker.skillId}\u0000${blocker.agentId}`
}

function isManagedCopyBlocker(blocker: ConflictBlocker) {
  return blocker.reason.startsWith('Managed copy ')
}

function suggestedAgentNames(ids: string[], agents: AgentSummary[]) {
  return ids.map((id) => agents.find((agent) => agent.id === id && agent.isCustom === false)?.displayName ?? id).join(', ')
}

function replacePackMemberId(manifest: SkillPackManifest, skillId: string, destinationId: string): SkillPackManifest {
  const replaceId = (id: string) => id === skillId ? destinationId : id
  const replaceKeys = <T,>(entries: Record<string, T> | undefined) =>
    Object.fromEntries(Object.entries(entries ?? {}).map(([id, value]) => [replaceId(id), value]))
  return {
    ...manifest,
    skillIds: manifest.skillIds.map(replaceId),
    skillFingerprints: replaceKeys(manifest.skillFingerprints),
    skillSources: replaceKeys(manifest.skillSources),
  }
}

export function SkillPackPage() {
  const { t } = useTranslation()
  const state = useSkillStoreV2()
  const [packQuery, setPackQuery] = useState('')
  const [builderDraft, setBuilder] = useState<{ mode: BuilderMode; runtime: PackRuntime } | null>(null)
  const [applyDraft, setApplyDraft] = useState<{
    pack: SkillPackDetail
    runtime: PackRuntime
    suggestedAgentIds: string[]
  } | null>(null)
  const [removalDraft, setRemovalReview] = useState<PackRemovalReview | null>(null)
  const [removalOperation, setRemovalOperation] = useState<{ runtime: PackRuntime } | null>(null)
  const [syncConflictDraft, setSyncConflict] = useState<PackSyncConflictState | null>(null)
  const [detailSkillId, setDetailSkillId] = useState<string | null>(null)
  const [actionBusy, setBusy] = useState(false)
  const [foregroundSync, setForegroundSync] = useState<{ runtime: PackRuntime } | null>(null)
  const [notice, setNotice] = useState<{ revision: number; packId?: string; message: string } | null>(null)
  const syncRequest = useRef(0)
  const [packImportDraft, setPackImport] = useState<PackImport | null>(null)
  const [packLinkDraft, setPackLink] = useState<PackLink | null>(null)
  const [packImportError, setPackImportError] = useState<string | null>(null)
  const [restoringMember, setRestoringMember] = useState<{ runtime: PackRuntime; skillId: string } | null>(null)
  const restoreRuntime = useRef<PackRuntime | null>(null)
  const runtimeRevision = useRuntimeEnvironmentStore((runtime) => runtime.selectionRevision)
  const application = applyDraft?.runtime.selectionRevision === runtimeRevision
    && applyDraft.runtime.selectedEnvironmentId === state.runtimeEnvironmentId
    && applyDraft.pack.id === state.selectedPackId
    ? applyDraft
    : null
  const builder = builderDraft?.runtime.selectionRevision === runtimeRevision ? builderDraft : null
  const builderMode = builder?.mode
  const syncConflict = syncConflictDraft?.runtime.selectionRevision === runtimeRevision && syncConflictDraft.packId === state.selectedPackId
    ? syncConflictDraft
    : null
  const removalReview = removalDraft?.runtime.selectionRevision === runtimeRevision && removalDraft.preview.packId === state.selectedPackId
    ? removalDraft
    : null
  const busy = actionBusy || foregroundSync?.runtime.selectionRevision === runtimeRevision
    || removalOperation?.runtime.selectionRevision === runtimeRevision
  const syncNotice = notice?.revision === runtimeRevision && (!notice.packId || notice.packId === state.selectedPackId)
    ? notice.message
    : null
  const packImport = packImportDraft?.runtime.selectionRevision === runtimeRevision ? packImportDraft : null
  const importAgentIds = packImport?.manifest.suggestedAgentIds ?? []
  const chooseAgentsAfterImport = importAgentIds.length > 0 && Boolean(packImport?.manifest.skillIds.length)
  const packLink = packLinkDraft?.runtime.selectionRevision === runtimeRevision ? packLinkDraft : null
  const activeRestore = restoringMember?.runtime.selectionRevision === runtimeRevision ? restoringMember : null
  const packImportBusy = busy || Boolean(activeRestore)

  useEffect(() => {
    if (state.initialized && !state.startupScanInFlight) void state.loadOverview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (state.selectedPackId || builderMode || state.packs.length === 0) return
    state.selectPack(state.packs[0].id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.packs, state.selectedPackId, builderMode])

  const filteredPacks = useMemo(() => {
    const q = packQuery.trim().toLowerCase()
    if (!q) return state.packs
    return state.packs.filter((pack) =>
      [pack.name, pack.description, pack.tags.join(' ')]
        .join(' ')
        .toLowerCase()
        .includes(q),
    )
  }, [packQuery, state.packs])

  const detail = state.selectedPackDetail?.id === state.selectedPackId ? state.selectedPackDetail : null
  const packMemberStatuses = useMemo(() => {
    const result = new Map<string, PackMemberStatus>()
    if (!packImport) return result
    const { manifest, reviewed } = packImport
    if (!reviewed) {
      if (manifest.version >= 2) return result
      const existing = new Set(state.skills.map((skill) => skill.id))
      for (const id of manifest.skillIds) {
        if (!existing.has(id)) result.set(id, 'missing')
      }
      return result
    }
    const expected = new Map(Object.entries(manifest.skillFingerprints ?? {}))
    for (const member of reviewed) {
      if (member.missing) result.set(member.skillId, 'missing')
      else if (!member.fingerprint) result.set(member.skillId, 'unavailable')
      else if (!expected.has(member.skillId)) result.set(member.skillId, 'unrecorded')
      else result.set(member.skillId, expected.get(member.skillId) === member.fingerprint ? 'match' : 'different')
    }
    return result
  }, [packImport, state.skills])
  const importStatuses = [...packMemberStatuses.values()]
  const hasMissingPackSkills = importStatuses.includes('missing')
  const hasUnavailablePackSkills = importStatuses.includes('unavailable')
  const needsLocalCopyConfirmation = importStatuses.includes('different') || importStatuses.includes('unrecorded')
  const packImportNeedsReview = Boolean(packImport && packImport.manifest.version >= 2 && !packImport.reviewed)
  const packImportBlocked = packImportNeedsReview || hasMissingPackSkills || hasUnavailablePackSkills
    || (needsLocalCopyConfirmation && !packImport?.acceptLocalCopies)
  const pinnedSources = new Map(Object.entries(packImport?.manifest.skillSources ?? {}))
  const restorableIds = packImport?.manifest.skillIds.filter((id) =>
    packMemberStatuses.get(id) === 'missing' && pinnedSources.has(id),
  ) ?? []
  const conflictingPins = new Set([...pinnedSources.keys()].filter((id) => {
    const status = packMemberStatuses.get(id)
    return status === 'different' || status === 'unavailable'
  }))
  const packMemberIds = packImport?.manifest.skillIds
  const occupiedSkillIds = useMemo(() => new Set([
    ...(packMemberIds ?? []),
    ...state.skills.map((skill) => skill.id),
  ].map((id) => id.toLowerCase())), [packMemberIds, state.skills])
  const packMemberships = useSkillPackMemberships(state.packs, detail)
  const missingCount = detail?.members.filter((member) => member.missing).length || 0
  const totalMembers = state.packs.reduce((sum, pack) => sum + pack.memberCount, 0)
  const totalApplied = state.packs.reduce((sum, pack) => sum + pack.appliedAgentCount, 0)
  const unhealthyPacks = state.packs.filter((pack) => !pack.healthy).length
  const syncIssueCount = state.packs.reduce((sum, pack) => sum + (pack.pendingSyncCount || 0) + (pack.failedSyncCount || 0), 0)
  const issueCount = unhealthyPacks + missingCount + syncIssueCount

  const setSyncNotice = (message: string | null, packId?: string) =>
    setNotice(message === null ? null : { revision: runtimeRevision, packId, message })

  const startCreate = () => {
    state.selectPack(null)
    setBuilder({ mode: 'create', runtime: useRuntimeEnvironmentStore.getState() })
  }

  const startEdit = () => setBuilder({ mode: 'edit', runtime: useRuntimeEnvironmentStore.getState() })
  const startDuplicate = () => setBuilder({ mode: 'duplicate', runtime: useRuntimeEnvironmentStore.getState() })

  const preparePackReview = async (manifest: SkillPackManifest, runtime: PackRuntime, sourceUrl?: string) => {
    if (!isPackRuntimeCurrent(runtime)) return
    const reviewed = manifest.version >= 2
      ? await skillApiV2.getPackFingerprints(manifest.skillIds, runtime.selectedEnvironmentId)
      : null
    if (!reviewed) await state.loadOverview(true)
    if (isPackRuntimeCurrent(runtime)) {
      setPackImport({ manifest, sourceUrl, runtime, reviewed, acceptLocalCopies: false, restoreErrors: new Map(), restoredFrom: new Map() })
    }
  }

  const reviewPackManifest = async () => {
    const runtime = useRuntimeEnvironmentStore.getState()
    if (busy || !isPackRuntimeCurrent(runtime)) return
    setBusy(true)
    state.setError(null)
    setSyncNotice(null)
    setPackImportError(null)
    try {
      const path = await open({ filters: [{ name: 'JSON', extensions: ['json'] }] })
      if (typeof path !== 'string' || !isPackRuntimeCurrent(runtime)) return
      const manifest = await skillApiV2.readPackManifest(path)
      await preparePackReview(manifest, runtime)
    } catch (error) {
      if (isPackRuntimeCurrent(runtime)) state.setError(String(error))
    } finally {
      setBusy(false)
    }
  }

  const startPackLinkReview = () => {
    const runtime = useRuntimeEnvironmentStore.getState()
    if (packImportBusy || !isPackRuntimeCurrent(runtime)) return
    state.setError(null)
    setSyncNotice(null)
    setPackImportError(null)
    setPackLink({ runtime, url: '', loading: false, error: null })
  }

  const reviewPackLink = async () => {
    if (!packLink || packLink.loading || !isPackRuntimeCurrent(packLink.runtime)) return
    const { runtime } = packLink
    const url = packLink.url.trim()
    if (!url) return
    setPackLink({ ...packLink, loading: true, error: null })
    try {
      const { manifest, sourceUrl } = await skillApiV2.readPackManifestLink(url)
      await preparePackReview(manifest, runtime, sourceUrl)
      if (isPackRuntimeCurrent(runtime)) setPackLink(null)
    } catch (error) {
      if (isPackRuntimeCurrent(runtime)) setPackLink({ ...packLink, loading: false, error: String(error) })
    }
  }

  const refreshPackComparison = async () => {
    if (!packImport || packImportBusy) return
    const { manifest, runtime } = packImport
    if (!isPackRuntimeCurrent(runtime)) {
      setPackImport(null)
      return
    }
    setBusy(true)
    setPackImportError(null)
    try {
      const reviewed = await skillApiV2.getPackFingerprints(manifest.skillIds, runtime.selectedEnvironmentId)
      if (isPackRuntimeCurrent(runtime)) setPackImport({ ...packImport, reviewed, acceptLocalCopies: false })
    } catch (error) {
      if (isPackRuntimeCurrent(runtime)) setPackImportError(String(error))
    } finally {
      setBusy(false)
    }
  }

  const startPackCopy = (skillId: string) => {
    if (packImportBusy || !packImport || !conflictingPins.has(skillId)) return
    const base = `${skillId}-restored`
    let destinationId = base
    for (let suffix = 2; occupiedSkillIds.has(destinationId.toLowerCase()); suffix += 1) {
      destinationId = `${base}-${suffix}`
    }
    setPackImport({ ...packImport, acceptLocalCopies: false, copyDraft: { skillId, destinationId } })
  }

  const restorePackMembers = async (requestedIds: string[], copyId?: string) => {
    if (packImportBusy || !packImport?.reviewed) return
    const { manifest, runtime } = packImport
    if (!isPackRuntimeCurrent(runtime) || restoreRuntime.current?.selectionRevision === runtime.selectionRevision) return
    if (copyId && (requestedIds.length !== 1 || occupiedSkillIds.has(copyId.toLowerCase()))) return
    const eligible = copyId ? conflictingPins : new Set(restorableIds)
    const skillIds = requestedIds.filter((id) => eligible.has(id))
    if (!skillIds.length) return
    restoreRuntime.current = runtime
    setPackImportError(null)
    const restoreErrors = new Map(packImport.restoreErrors)
    let nextImport = packImport
    try {
      for (const skillId of skillIds) {
        if (!isPackRuntimeCurrent(runtime)) return
        setRestoringMember({ runtime, skillId })
        try {
          const restoreManifest = copyId ? replacePackMemberId(manifest, skillId, copyId) : manifest
          await skillApiV2.restorePackMember(restoreManifest, copyId ?? skillId, runtime.selectedEnvironmentId)
          restoreErrors.delete(skillId)
          if (copyId) {
            const restoredFrom = new Map(packImport.restoredFrom)
            restoredFrom.set(copyId, restoredFrom.get(skillId) ?? skillId)
            restoredFrom.delete(skillId)
            nextImport = { ...packImport, manifest: restoreManifest, restoredFrom, copyDraft: undefined }
          }
        } catch (error) {
          restoreErrors.set(skillId, String(error))
        }
      }
      if (!isPackRuntimeCurrent(runtime)) return
      const reviewed = await skillApiV2.getPackFingerprints(nextImport.manifest.skillIds, runtime.selectedEnvironmentId)
      if (isPackRuntimeCurrent(runtime)) {
        setPackImport({ ...nextImport, reviewed, restoreErrors, acceptLocalCopies: false })
        await state.loadOverview(true)
      }
    } catch (error) {
      if (isPackRuntimeCurrent(runtime)) {
        setPackImport({ ...nextImport, reviewed: null, restoreErrors, acceptLocalCopies: false })
        setPackImportError(String(error))
      }
    } finally {
      if (restoreRuntime.current === runtime) restoreRuntime.current = null
      setRestoringMember((current) => current?.runtime === runtime ? null : current)
    }
  }

  const importPackManifest = async () => {
    if (packImportBusy || !packImport || packImportBlocked) return
    const { manifest, runtime, reviewed, acceptLocalCopies } = packImport
    if (!isPackRuntimeCurrent(runtime)) {
      setPackImport(null)
      return
    }
    setBusy(true)
    setPackImportError(null)
    try {
      const pack = reviewed
        ? await skillApiV2.importReviewedPack(manifest, reviewed, acceptLocalCopies, runtime.selectedEnvironmentId)
        : await skillApiV2.upsertPack({
            id: '',
            name: manifest.name,
            description: manifest.description,
            tags: manifest.tags,
            skillIds: manifest.skillIds,
          }, { deferSync: true, environmentId: runtime.selectedEnvironmentId })
      if (!isPackRuntimeCurrent(runtime)) return
      setPackImport(null)
      setPackQuery('')
      useSkillStoreV2.setState({ selectedPackId: pack.id, selectedPackDetail: pack })
      setSyncNotice(t('skills.packManifest.imported', { name: pack.name }))
      await state.loadOverview(true)
      if (chooseAgentsAfterImport && isPackRuntimeCurrent(runtime)
        && useSkillStoreV2.getState().selectedPackId === pack.id) {
        setApplyDraft({ pack, runtime, suggestedAgentIds: importAgentIds })
      }
    } catch (error) {
      if (isPackRuntimeCurrent(runtime)) setPackImportError(String(error))
    } finally {
      setBusy(false)
    }
  }

  const exportPackManifest = async (pack: SkillPackDetail, mode: PackExportMode) => {
    const runtime = useRuntimeEnvironmentStore.getState()
    if (busy || !isPackRuntimeCurrent(runtime)) return
    const includeSources = mode !== 'definition'
    const appliedAgentIds = new Set(pack.appliedAgents.map((agent) => agent.agentId))
    const suggestedAgentIds = mode === 'migration'
      ? state.agents.filter((agent) => agent.isCustom === false
          && agent.id !== SHARED_SKILLS_AGENT_ID && appliedAgentIds.has(agent.id)).map((agent) => agent.id)
      : []
    setBusy(true)
    state.setError(null)
    setSyncNotice(null)
    try {
      const path = await save({
        defaultPath: 'skill-pack.agentbro-pack.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      })
      if (!path || !isPackRuntimeCurrent(runtime)) return
      const skillIds = pack.members.map((member) => member.skillId)
      if (includeSources) setSyncNotice(t('skills.packManifest.verifyingSources'))
      const fingerprints = includeSources
        ? await skillApiV2.getPackFingerprints(skillIds, runtime.selectedEnvironmentId, true)
        : await skillApiV2.getPackFingerprints(skillIds, runtime.selectedEnvironmentId)
      if (!isPackRuntimeCurrent(runtime)) return
      const sources = includeSources ? Object.fromEntries(fingerprints.flatMap(({ skillId, source }) =>
        source ? [[skillId, source]] : [],
      )) : {}
      const sourceCount = Object.keys(sources).length
      await skillApiV2.writePackManifest(path, {
        format: 'agentbro-skill-pack',
        version: suggestedAgentIds.length ? 4 : sourceCount ? 3 : 2,
        name: pack.name,
        description: pack.description,
        tags: pack.tags,
        skillIds,
        skillFingerprints: Object.fromEntries(fingerprints.flatMap(({ skillId, fingerprint }) =>
          fingerprint ? [[skillId, fingerprint]] : [],
        )),
        ...(sourceCount ? { skillSources: sources } : {}),
        ...(suggestedAgentIds.length ? { suggestedAgentIds } : {}),
      })
      if (isPackRuntimeCurrent(runtime)) {
        if (includeSources) {
          setSyncNotice(sourceCount
            ? t('skills.packManifest.exportedWithVerifiedSources', { verifiedCount: sourceCount, totalCount: skillIds.length })
            : t('skills.packManifest.exportedWithoutSources'))
        } else {
          setSyncNotice(t('skills.packManifest.exported'))
        }
      }
    } catch (error) {
      if (isPackRuntimeCurrent(runtime)) {
        setSyncNotice(null)
        state.setError(String(error))
      }
    } finally {
      setBusy(false)
    }
  }

  const refreshSelectedPack = async (packId: string, runtime: PackRuntime) => {
    if (!isPackRuntimeCurrent(runtime)) return
    await state.loadOverview(true)
    if (isPackRuntimeCurrent(runtime) && useSkillStoreV2.getState().selectedPackId === packId) {
      await state.selectPack(packId)
    }
  }

  const reviewPackRemoval = async (packId: string, load: (environmentId: string) => Promise<PackRemoval>) => {
    const runtime = useRuntimeEnvironmentStore.getState()
    if (busy || !isPackRuntimeCurrent(runtime)) return
    const operation = { runtime }
    const isReviewCurrent = () => isPackRuntimeCurrent(runtime) && useSkillStoreV2.getState().selectedPackId === packId
    setRemovalOperation(operation)
    try {
      const removal = await load(runtime.selectedEnvironmentId)
      if (isReviewCurrent()) setRemovalReview({ ...removal, runtime })
    } catch (error) {
      if (isReviewCurrent()) state.setError(String(error))
    } finally {
      setRemovalOperation((current) => current === operation ? null : current)
    }
  }

  const executePackRemoval = async (review: PackRemovalReview, alsoRemoveTargets = false) => {
    const { runtime } = review
    const { packId } = review.preview
    const environmentId = runtime.selectedEnvironmentId
    const isSelected = () => useSkillStoreV2.getState().selectedPackId === packId
    if (busy || !isPackRuntimeCurrent(runtime) || !isSelected()) return
    const operation = { runtime }
    setRemovalOperation(operation)
    try {
      switch (review.kind) {
        case 'delete':
          await skillApiV2.deletePack(packId, environmentId)
          break
        case 'member':
          await skillApiV2.removeSkillFromPack(packId, review.preview.skillId, alsoRemoveTargets, environmentId)
          break
        case 'revoke':
          await skillApiV2.removePackFromAgent(packId, review.preview.agentId, environmentId)
          break
      }
      if (!isPackRuntimeCurrent(runtime)) return
      setRemovalReview((current) => current === review ? null : current)
      if (review.kind === 'delete') {
        await state.loadOverview(true)
        if (isPackRuntimeCurrent(runtime) && isSelected()) await state.selectPack(null)
        return
      }
      await refreshSelectedPack(packId, runtime)
      if (review.kind === 'revoke' && isPackRuntimeCurrent(runtime)
        && useSkillStoreV2.getState().selectedAgentId === review.preview.agentId) {
        await state.loadAgentDetail(review.preview.agentId, true)
      }
    } catch (error) {
      if (isPackRuntimeCurrent(runtime) && isSelected()) state.setError(String(error))
    } finally {
      setRemovalOperation((current) => current === operation ? null : current)
    }
  }

  const syncPack = async (
    packId: string,
    agentIds: string[] = [],
    options: SyncPackOptions = {},
    runtime: PackRuntime = useRuntimeEnvironmentStore.getState(),
  ) => {
    if (!isPackRuntimeCurrent(runtime)) return
    const request = { runtime }
    const revision = ++syncRequest.current
    const isLatest = () => revision === syncRequest.current && isPackRuntimeCurrent(runtime)
    const isSelected = () => useSkillStoreV2.getState().selectedPackId === packId
    if (options.background) {
      setSyncNotice(`“${options.packName || '技能包'}”已保存，正在后台同步到 Agent…`, packId)
    } else {
      setForegroundSync(request)
      setSyncNotice(null)
    }
    try {
      const result = await skillApiV2.syncPackToAgents(packId, agentIds, runtime.selectedEnvironmentId)
      if (!isLatest()) {
        await refreshSelectedPack(packId, runtime)
        return
      }
      const failedAgents = result.agents.filter((agent) => agent.status === 'failed')
      if (failedAgents.length > 0 && isSelected()) {
        const targetAgents = failedAgents.map((agent) => agent.agentId)
        const requestedMode = state.settings?.defaultDistributeMode || 'link'
        const preview = await skillApiV2.previewApplyPack(packId, targetAgents, requestedMode, runtime.selectedEnvironmentId)
        if (!isLatest()) {
          await refreshSelectedPack(packId, runtime)
          return
        }
        if (preview.blockers.length > 0 && isSelected()) {
          setSyncConflict({ packId, runtime, targetAgents, requestedMode, preview })
          setSyncNotice('同步遇到需要处理的冲突，请选择处理方式。', packId)
          await refreshSelectedPack(packId, runtime)
          return
        }
      }
      setSyncNotice(
        failedAgents.length > 0
          ? t('skills.packSyncFailedNotice', { count: failedAgents.length, defaultValue: '{{count}} 个 Agent 同步失败，可查看状态后重试。' })
          : options.background
            ? `“${options.packName || '技能包'}”已保存，并同步到 ${result.agents.length} 个 Agent。`
          : t('skills.packSyncDoneNotice', { count: result.agents.length, defaultValue: '已同步 {{count}} 个 Agent。' }),
        packId,
      )
      await refreshSelectedPack(packId, runtime)
    } catch (e) {
      if (!isLatest()) return
      if (isSelected()) state.setError(String(e))
      if (options.background) setSyncNotice(`“${options.packName || '技能包'}”已保存，但后台同步失败。`, packId)
    } finally {
      if (!options.background) setForegroundSync((current) => current === request ? null : current)
    }
  }

  return (
    <div className="sm2 sm2--packs sm2--packs-redesign">
      <div className="sm2__header sm2__header--stacked">
        <div>
          <h2 className="sm2__title">技能包</h2>
          <p className="sm2__header-subtitle">把常用 Skills 组合起来，一次应用到多个 Agent。</p>
          <div className="sm2__pack-page-meta" aria-label="技能包概览">
            <span><strong>{state.packs.length}</strong> 个组合</span>
            <span><strong>{totalMembers}</strong> 个成员引用</span>
            <span><strong>{totalApplied}</strong> 个 Agent 应用</span>
            <span className={issueCount > 0 ? 'sm2__pack-page-meta--warn' : 'sm2__pack-page-meta--ok'}>
              <i aria-hidden="true" />{issueCount > 0 ? `${issueCount} 项待处理` : '状态正常'}
            </span>
          </div>
        </div>
        <div className="sm2__tabs">
          <button className="sm2__btn sm2__btn--primary" onClick={startCreate}>＋ 新建技能包</button>
          <button className="sm2__btn" disabled={busy || state.loading} onClick={reviewPackManifest}>
            {t('skills.packManifest.import')}
          </button>
          <button className="sm2__btn" disabled={busy || state.loading} onClick={startPackLinkReview}>
            {t('skills.packManifest.importFromLink')}
          </button>
          <button className="sm2__btn" onClick={() => state.loadOverview(true)} disabled={state.loading || busy}>
            {state.loading ? '刷新中…' : '刷新'}
          </button>
        </div>
      </div>

      {state.error && <div className="sm2__error">{state.error}</div>}
      {syncNotice && <div className="sm2__notice sm2__notice--ok">{syncNotice}</div>}

      <div className="sm2__pack-layout">
        <aside className="sm2__pack-sidebar settings-scroll">
          <div className="sm2__pack-sidebar-head">
            <div>
              <strong>全部技能包</strong>
              <span>选择一个组合查看详情</span>
            </div>
            <span>{filteredPacks.length}</span>
          </div>
          <input
            className="sm2__search sm2__search--full"
            value={packQuery}
            onChange={(e) => setPackQuery(e.target.value)}
            placeholder="搜索名称、描述或标签"
          />

          {state.packs.length === 0 ? (
            <div className="sm2__pack-empty-state">
              <strong>还没有技能包</strong>
              <span>从中心库挑选 Skills，保存成可复用组合。</span>
              <button className="sm2__btn sm2__btn--primary" onClick={startCreate}>创建第一个</button>
            </div>
          ) : filteredPacks.length === 0 ? (
            <div className="sm2__empty sm2__empty--compact">没有匹配的技能包</div>
          ) : (
            <div className="sm2__pack-list">
              {filteredPacks.map((pack) => (
                <PackListItem
                  key={pack.id}
                  pack={pack}
                  active={state.selectedPackId === pack.id && !builderMode}
                  onClick={() => {
                    setBuilder(null)
                    setApplyDraft(null)
                    setSyncConflict(null)
                    setRemovalReview(null)
                    state.selectPack(pack.id)
                  }}
                />
              ))}
            </div>
          )}
        </aside>

        <main className="sm2__pack-canvas settings-scroll">
          {!detail ? (
            <PackLanding onCreate={startCreate} />
          ) : (
            <PackDetail
              key={detail.id}
              detail={detail}
              busy={busy}
              onApply={() => {
                const runtime = useRuntimeEnvironmentStore.getState()
                if (isPackRuntimeCurrent(runtime) && runtime.selectionRevision === runtimeRevision) {
                  setApplyDraft({ pack: detail, runtime, suggestedAgentIds: [] })
                }
              }}
              onEdit={startEdit}
              onExport={(mode) => exportPackManifest(detail, mode)}
              onDuplicate={startDuplicate}
              onDelete={() => reviewPackRemoval(detail.id, async (environmentId) => ({
                kind: 'delete', preview: await skillApiV2.previewDeletePack(detail.id, environmentId),
              }))}
              onRemoveSkill={(skillId) => reviewPackRemoval(detail.id, async (environmentId) => ({
                kind: 'member', preview: await skillApiV2.previewRemoveSkillFromPack(detail.id, skillId, environmentId),
              }))}
              onOpenSkill={setDetailSkillId}
              onRevoke={(agentId) => reviewPackRemoval(detail.id, async (environmentId) => ({
                kind: 'revoke', preview: await skillApiV2.previewRemovePackFromAgent(detail.id, agentId, environmentId),
              }))}
              onSync={(agentIds) => syncPack(detail.id, agentIds)}
              membershipsBySkillId={packMemberships.membershipsBySkillId}
              membershipsLoading={packMemberships.loading}
            />
          )}
        </main>
      </div>

      <SkillDetailSlider
        skillId={detailSkillId}
        open={Boolean(detailSkillId)}
        onClose={() => setDetailSkillId(null)}
      />

      {packLink && (
        <PreviewDialog
          title={t('skills.packManifest.importFromLink')}
          confirmLabel={t('skills.packManifest.reviewLink')}
          cancelLabel={t('settings.cancel')}
          busyLabel={t('skills.packManifest.readingLink')}
          busy={packLink.loading}
          disabled={!packLink.url.trim()}
          onConfirm={reviewPackLink}
          onCancel={() => setPackLink(null)}
        >
          <p id="sm2-pack-link-hint">{t('skills.packManifest.linkHint')}</p>
          <div className="sm2__field">
            <label htmlFor="sm2-pack-link">{t('skills.packManifest.link')}</label>
            <input
              id="sm2-pack-link"
              type="url"
              aria-describedby="sm2-pack-link-hint"
              autoFocus
              value={packLink.url}
              disabled={packLink.loading}
              onChange={(event) => setPackLink({ ...packLink, url: event.target.value, error: null })}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.nativeEvent.isComposing) void reviewPackLink()
              }}
            />
          </div>
          {packLink.error && <div className="sm2__error" role="alert">{packLink.error}</div>}
        </PreviewDialog>
      )}

      {packImport && (
        <PreviewDialog
          title={t('skills.packManifest.import')}
          confirmLabel={t(chooseAgentsAfterImport ? 'skills.packManifest.createAndChooseAgents' : 'skills.packManifest.create')}
          cancelLabel={t('settings.cancel')}
          busyLabel={t(activeRestore ? 'skills.packManifest.restoring' : 'skills.packManifest.creating')}
          busy={packImportBusy}
          disabled={packImportBlocked}
          onConfirm={importPackManifest}
          onCancel={() => { if (!packImportBusy) setPackImport(null) }}
        >
          <h4>{packImport.manifest.name}</h4>
          {packImport.sourceUrl && (
            <p className="sm2__pack-manifest-url">{t('skills.packManifest.linkSource')} <code>{packImport.sourceUrl}</code></p>
          )}
          {packImport.manifest.description && <p>{packImport.manifest.description}</p>}
          {packImport.manifest.tags.length > 0 && <p>{packImport.manifest.tags.join(', ')}</p>}
          <p>{t(Object.keys(packImport.manifest.skillSources ?? {}).length
            ? 'skills.packManifest.pinnedReview' : 'skills.packManifest.review')}</p>
          {importAgentIds.length > 0 && (
            <p>{t('skills.packManifest.suggestedAgents', {
              agents: suggestedAgentNames(importAgentIds, state.agents),
            })}</p>
          )}
          {hasMissingPackSkills && <div className="sm2__notice sm2__notice--warn">{t('skills.packManifest.missingHelp')}</div>}
          {hasUnavailablePackSkills && <div className="sm2__notice sm2__notice--warn">{t('skills.packManifest.unavailableHelp')}</div>}
          {restorableIds.length > 1 && (
            <button className="sm2__btn" type="button" disabled={packImportBusy} onClick={() => void restorePackMembers(restorableIds)}>
              {t('skills.packManifest.restoreMissing', { count: restorableIds.length })}
            </button>
          )}
          {packImport.manifest.skillIds.length ? (
            <ul className="sm2__pack-manifest-members">
              {packImport.manifest.skillIds.map((id) => {
                const status = packMemberStatuses.get(id)
                const canRestoreCopy = conflictingPins.has(id)
                const source = status === 'missing' || canRestoreCopy ? pinnedSources.get(id) : undefined
                const copyDraft = canRestoreCopy && packImport.copyDraft?.skillId === id ? packImport.copyDraft : undefined
                const copyId = copyDraft?.destinationId.trim() ?? ''
                const copyIdTaken = occupiedSkillIds.has(copyId.toLowerCase())
                const originalId = packImport.restoredFrom.get(id)
                const restoreError = status === 'match' ? undefined : packImport.restoreErrors.get(id)
                return (
                  <li key={id} className="sm2__change">
                    <code>{id}</code>
                    {originalId && <span className="sm2__muted">{t('skills.packManifest.restoredFrom', { name: originalId })}</span>}
                    {status && (
                      <span className={status === 'match' ? 'sm2__tag' : 'sm2__tag sm2__tag--warn'}>
                        {t(`skills.packManifest.${status}`)}
                      </span>
                    )}
                    {source && (
                      <div className="sm2__pack-manifest-source">
                        <code title={source.commit}>{source.repository}@{source.commit.slice(0, 12)} · {source.skillPath}</code>
                        {status === 'missing' && (
                          <button className="sm2__btn" type="button" aria-label={t('skills.packManifest.restoreMember', { name: id })}
                            disabled={packImportBusy} onClick={() => void restorePackMembers([id])}>
                            {t(activeRestore?.skillId === id ? 'skills.packManifest.restoring' : 'skills.packManifest.restore')}
                          </button>
                        )}
                        {canRestoreCopy && !copyDraft && (
                          <button className="sm2__btn" type="button" aria-label={t('skills.packManifest.restoreCopyOf', { name: id })}
                            disabled={packImportBusy} onClick={() => startPackCopy(id)}>
                            {t('skills.packManifest.restoreAsCopy')}
                          </button>
                        )}
                      </div>
                    )}
                    {copyDraft && (
                      <div className="sm2__pack-manifest-copy">
                        <label className="sm2__field">
                          <span>{t('skills.packManifest.copyId')}</span>
                          <input autoFocus value={copyDraft.destinationId} disabled={packImportBusy} aria-invalid={copyIdTaken || undefined}
                            onChange={(event) => setPackImport({ ...packImport, copyDraft: { skillId: id, destinationId: event.target.value } })} />
                        </label>
                        <button className="sm2__btn" type="button" disabled={packImportBusy || !copyId || copyIdTaken}
                          onClick={() => void restorePackMembers([id], copyId)}>
                          {t(activeRestore?.skillId === id ? 'skills.packManifest.restoring' : 'skills.packManifest.restoreCopy')}
                        </button>
                        <button className="sm2__btn" type="button" disabled={packImportBusy}
                          onClick={() => setPackImport({ ...packImport, copyDraft: undefined })}>
                          {t('settings.cancel')}
                        </button>
                        {copyIdTaken && (
                          <div className="sm2__error sm2__pack-manifest-member-error" role="alert">{t('skills.packManifest.copyIdInUse')}</div>
                        )}
                      </div>
                    )}
                    {restoreError && <div className="sm2__error sm2__pack-manifest-member-error" role="alert">{restoreError}</div>}
                  </li>
                )
              })}
            </ul>
          ) : <p>{t('skills.packManifest.empty')}</p>}
          {needsLocalCopyConfirmation && (
            <label className="sm2__checkbox-row">
              <input
                type="checkbox"
                checked={packImport.acceptLocalCopies}
                disabled={packImportBusy || hasMissingPackSkills || hasUnavailablePackSkills}
                onChange={(event) => setPackImport({ ...packImport, acceptLocalCopies: event.target.checked })}
              />
              {t('skills.packManifest.useLocalCopies')}
            </label>
          )}
          {packImport.manifest.version >= 2 && (
            <button className="sm2__btn" type="button" disabled={packImportBusy} onClick={refreshPackComparison}>
              {t('skills.packManifest.recheck')}
            </button>
          )}
          {packImportError && <div className="sm2__error" role="alert">{packImportError}</div>}
        </PreviewDialog>
      )}

      {builder && (
        <PackBuilderDialog
          mode={builder.mode}
          runtime={builder.runtime}
          existing={builder.mode === 'create' ? null : detail}
          membershipsBySkillId={packMemberships.membershipsBySkillId}
          membershipsLoading={packMemberships.loading}
          onCancel={() => setBuilder(null)}
          onSaved={(saved, options) => {
            if (!isPackRuntimeCurrent(builder.runtime)) return
            setBuilder((current) => current === builder ? null : current)
            useSkillStoreV2.setState({
              selectedPackId: saved.id,
              selectedPackDetail: saved,
            })
            if ((options.forceSync || state.settings?.autoSyncSkillPacks !== false) && packNeedsSync(saved)) {
              void syncPack(saved.id, [], { background: true, packName: saved.name }, builder.runtime)
              return
            }
            setSyncNotice(`“${saved.name}”已保存。`, saved.id)
            void refreshSelectedPack(saved.id, builder.runtime)
          }}
        />
      )}

      {application && (
        <ApplyPackDialog
          key={application.pack.id}
          pack={application.pack}
          runtime={application.runtime}
          suggestedAgentIds={application.suggestedAgentIds}
          agents={state.agents}
          defaultMode={state.settings?.defaultDistributeMode || 'link'}
          onClose={() => setApplyDraft((current) => current === application ? null : current)}
          onDone={async () => {
            setApplyDraft((current) => current === application ? null : current)
            await refreshSelectedPack(application.pack.id, application.runtime)
          }}
        />
      )}
      {removalReview?.kind === 'delete' && (
        <DeletePackDialog
          preview={removalReview.preview}
          busy={busy}
          onClose={() => setRemovalReview(null)}
          onDelete={() => executePackRemoval(removalReview)}
        />
      )}
      {removalReview?.kind === 'member' && (
        <RemoveSkillFromPackDialog
          preview={removalReview.preview}
          busy={busy}
          onClose={() => setRemovalReview(null)}
          onExecute={(alsoRemoveTargets) => executePackRemoval(removalReview, alsoRemoveTargets)}
        />
      )}
      {removalReview?.kind === 'revoke' && (
        <RevokePackDialog
          preview={removalReview.preview}
          busy={busy}
          onClose={() => setRemovalReview(null)}
          onExecute={() => executePackRemoval(removalReview)}
        />
      )}
      {syncConflict && (
        <PackSyncConflictDialog
          key={`${syncConflict.packId}:${syncConflict.targetAgents.join(',')}`}
          conflict={syncConflict}
          onClose={() => setSyncConflict(null)}
          onDone={async () => {
            setSyncConflict((current) => current === syncConflict ? null : current)
            setSyncNotice(null)
            await refreshSelectedPack(syncConflict.packId, syncConflict.runtime)
          }}
        />
      )}
    </div>
  )
}

function PackBuilderDialog({
  mode,
  runtime,
  existing,
  membershipsBySkillId,
  membershipsLoading,
  onCancel,
  onSaved,
}: {
  mode: BuilderMode
  runtime: PackRuntime
  existing: SkillPackDetail | null
  membershipsBySkillId: SkillPackMembershipsBySkillId
  membershipsLoading: boolean
  onCancel: () => void
  onSaved: (pack: SkillPackDetail, options: PackBuilderSaveOptions) => void
}) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onCancel()
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [onCancel])

  if (typeof document === 'undefined') return null

  const title = mode === 'edit' ? '编辑技能包' : mode === 'duplicate' ? '复制技能包' : '创建技能包'

  return createPortal(
    <div className="sm2__overlay sm2__pack-builder-overlay" onClick={onCancel}>
      <div
        className="sm2__modal sm2__pack-builder-modal"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onClick={(e) => e.stopPropagation()}
      >
        <PackBuilderPanel
          mode={mode}
          runtime={runtime}
          existing={existing}
          membershipsBySkillId={membershipsBySkillId}
          membershipsLoading={membershipsLoading}
          onCancel={onCancel}
          onSaved={onSaved}
        />
      </div>
    </div>,
    document.body,
  )
}

function PackMetric({ label, value, tone }: { label: string; value: number; tone?: 'ok' | 'warn' }) {
  return (
    <div className={`sm2__pack-metric${tone ? ` sm2__pack-metric--${tone}` : ''}`}>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  )
}

function PackListItem({
  pack,
  active,
  onClick,
}: {
  pack: SkillPackSummary
  active: boolean
  onClick: () => void
}) {
  const isDefault = pack.id === DEFAULT_SKILL_PACK_ID
  const syncStatus = pack.syncStatus || 'synced'
  const hasSyncIssue = (pack.pendingSyncCount || 0) + (pack.failedSyncCount || 0) > 0
  return (
    <button className={`sm2__pack-list-item${active ? ' sm2__pack-list-item--active' : ''}${isDefault ? ' sm2__pack-list-item--default' : ''}`} onClick={onClick}>
      <PackEmblem compact builtIn={isDefault} />
      <span className="sm2__pack-list-body">
        <span className="sm2__pack-list-top">
          <strong>{pack.name}</strong>
          <span className={`sm2__pack-health ${pack.healthy && !hasSyncIssue ? 'sm2__pack-health--ok' : 'sm2__pack-health--warn'}`}>
            {isDefault ? '内置' : hasSyncIssue ? packSyncStatusLabel(syncStatus) : pack.healthy ? '正常' : '有缺失'}
          </span>
        </span>
        <span className="sm2__pack-list-desc">{pack.description || '自定义 Skill 组合'}</span>
        <span className="sm2__pack-list-meta">
          <span>{pack.memberCount} Skills</span>
          <span>{pack.appliedAgentCount} Agents</span>
          {hasSyncIssue && <span>{(pack.pendingSyncCount || 0) + (pack.failedSyncCount || 0)} 待同步</span>}
        </span>
      </span>
      <span className="sm2__pack-list-arrow" aria-hidden="true">›</span>
    </button>
  )
}

function PackEmblem({ compact = false, builtIn = false }: { compact?: boolean; builtIn?: boolean }) {
  return (
    <span className={`sm2__pack-emblem${compact ? ' sm2__pack-emblem--compact' : ''}${builtIn ? ' sm2__pack-emblem--built-in' : ''}`} aria-hidden="true">
      <i />
      <i />
      <i />
    </span>
  )
}

function PackLanding({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="sm2__pack-landing">
      <div className="sm2__pack-landing-mark">PACK</div>
      <strong>选择一个技能包</strong>
      <span>右侧会显示包内成员、已应用 Agent 和可执行操作。</span>
      <button className="sm2__btn sm2__btn--primary" onClick={onCreate}>新建技能包</button>
    </div>
  )
}

function PackDetail({
  detail,
  busy,
  membershipsBySkillId,
  membershipsLoading,
  onApply,
  onEdit,
  onExport,
  onDuplicate,
  onDelete,
  onRemoveSkill,
  onOpenSkill,
  onRevoke,
  onSync,
}: {
  detail: SkillPackDetail
  busy: boolean
  membershipsBySkillId: SkillPackMembershipsBySkillId
  membershipsLoading: boolean
  onApply: () => void
  onEdit: () => void
  onExport: (mode: PackExportMode) => void
  onDuplicate: () => void
  onDelete: () => void
  onRemoveSkill: (skillId: string) => void
  onOpenSkill: (skillId: string) => void
  onRevoke: (agentId: string) => void
  onSync: (agentIds?: string[]) => void
}) {
  const { t } = useTranslation()
  const missing = detail.members.filter((member) => member.missing)
  const isDefault = detail.id === DEFAULT_SKILL_PACK_ID
  const syncStatus = detail.syncStatus || 'synced'
  const needsSync = packNeedsSync(detail)
  const [activeSection, setActiveSection] = useState<'members' | 'agents'>(needsSync ? 'agents' : 'members')
  const [memberQuery, setMemberQuery] = useState('')
  const filteredMembers = useMemo(() => {
    const query = memberQuery.trim().toLowerCase()
    if (!query) return detail.members
    return detail.members.filter((member) =>
      [member.skillName, member.skillId].join(' ').toLowerCase().includes(query),
    )
  }, [detail.members, memberQuery])

  return (
    <div className="sm2__pack-workbench">
      <header className="sm2__pack-detail-hero">
        <div className="sm2__pack-detail-identity">
          <PackEmblem builtIn={isDefault} />
          <div className="sm2__pack-detail-main">
            <div className="sm2__pack-kicker">{isDefault ? '系统组合' : '自定义组合'}</div>
            <h3>{detail.name}</h3>
            <p>{detail.description || '保存一组中心库 Skill，方便重复应用。'}</p>
            <div className="sm2__pack-tagline">
              <span>{detail.members.length} Skills</span>
              <span>{detail.appliedAgents.length} Agents</span>
              {isDefault && <span>自动更新成员</span>}
              {!isDefault && detail.tags.map((tag) => <span key={tag}>{tag}</span>)}
            </div>
          </div>
        </div>
        <div className="sm2__pack-actions">
          <button className="sm2__btn sm2__btn--primary" onClick={onApply} disabled={busy || detail.members.length === 0 || missing.length > 0}>
            应用到 Agent
          </button>
          {!isDefault && <button className="sm2__btn" onClick={onEdit} disabled={busy}>编辑</button>}
          {!isDefault && needsSync && (
            <button className="sm2__btn" onClick={() => onSync()} disabled={busy}>
              {busy ? '同步中…' : '同步全部'}
            </button>
          )}
          <details className="sm2__pack-more">
            <summary aria-label="更多操作">•••</summary>
            <div role="menu">
              <button role="menuitem" onClick={() => onExport('definition')} disabled={busy}>{t('skills.packManifest.export')}</button>
              <button role="menuitem" onClick={() => onExport('sources')} disabled={busy}>{t('skills.packManifest.exportWithSources')}</button>
              <button role="menuitem" onClick={() => onExport('migration')} disabled={busy}>{t('skills.packManifest.exportForMigration')}</button>
              {!isDefault && (
                <>
                  <button role="menuitem" onClick={onDuplicate} disabled={busy}>复制技能包</button>
                  <button role="menuitem" className="sm2__pack-more-danger" onClick={onDelete} disabled={busy}>删除技能包</button>
                </>
              )}
            </div>
          </details>
        </div>
      </header>

      {isDefault && (
        <div className="sm2__pack-system-note">
          <strong>始终跟随中心库</strong>
          <span>这是系统内置入口，成员会自动保持为中心库全量；可以应用或撤销，但不需要手动维护。</span>
        </div>
      )}

      {missing.length > 0 && (
        <div className="sm2__notice sm2__notice--warn">
          {missing.length} 个成员已不在中心库中。应用前需要先恢复或从技能包移除。
        </div>
      )}

      {!isDefault && needsSync && (
        <div className={`sm2__notice ${syncStatus === 'failed' || syncStatus === 'partial' ? 'sm2__notice--warn' : 'sm2__notice--ok'}`}>
          {packSyncStatusLabel(syncStatus)}：{detail.pendingSyncCount || 0} 个待同步，{detail.failedSyncCount || 0} 个失败。
        </div>
      )}

      <div className="sm2__pack-view-tabs" role="tablist" aria-label="技能包详情">
        <button
          type="button"
          role="tab"
          aria-selected={activeSection === 'members'}
          className={activeSection === 'members' ? 'sm2__pack-view-tab sm2__pack-view-tab--active' : 'sm2__pack-view-tab'}
          onClick={() => setActiveSection('members')}
        >
          包内 Skills <span>{detail.members.length}</span>
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeSection === 'agents'}
          className={activeSection === 'agents' ? 'sm2__pack-view-tab sm2__pack-view-tab--active' : 'sm2__pack-view-tab'}
          onClick={() => setActiveSection('agents')}
        >
          已应用 Agent <span>{detail.appliedAgents.length}</span>
          {needsSync && <i aria-label="需要同步" />}
        </button>
      </div>

      <div className="sm2__pack-tab-panel" role="tabpanel">
        {activeSection === 'members' ? (
          <section className="sm2__pack-section sm2__pack-section--members">
          <div className="sm2__pack-section-head">
            <div>
              <h3>包内 Skills</h3>
              <span>应用技能包时，会把这些成员一起分发。</span>
            </div>
            <input
              className="sm2__search sm2__pack-member-search"
              value={memberQuery}
              onChange={(event) => setMemberQuery(event.target.value)}
              aria-label="搜索包内 Skills"
              placeholder="搜索包内 Skills"
            />
          </div>
          {detail.members.length === 0 ? (
            <div className="sm2__pack-panel-empty">
              <strong>这个技能包还是空的</strong>
              <span>{isDefault ? '中心库中暂时没有 Skills。' : '点击“编辑”添加需要一起使用的 Skills。'}</span>
              {!isDefault && <button className="sm2__btn" onClick={onEdit}>添加 Skills</button>}
            </div>
          ) : filteredMembers.length === 0 ? (
            <div className="sm2__pack-panel-empty">
              <strong>没有匹配的 Skill</strong>
              <span>换个名称或 Skill ID 试试。</span>
            </div>
          ) : (
            <div className="sm2__pack-member-list">
              {filteredMembers.map((member) => (
                <div key={member.skillId} className={`sm2__pack-member-row${member.missing ? ' sm2__pack-member-row--missing' : ''}`}>
                  <button
                    type="button"
                    className="sm2__pack-member-open"
                    aria-label={`查看 Skill 详情 ${member.skillName}`}
                    disabled={member.missing}
                    onClick={() => onOpenSkill(member.skillId)}
                  >
                    <span className="sm2__pack-skill-mark" aria-hidden="true">{member.skillName.trim().slice(0, 1).toUpperCase() || 'S'}</span>
                    <span className="sm2__pack-member-body">
                      <strong>{member.skillName}</strong>
                      <span>{member.skillId}</span>
                    </span>
                    <SkillPackMembershipTags
                      packs={membershipsBySkillId[member.skillId] || []}
                      loading={membershipsLoading}
                      excludePackId={isDefault ? undefined : detail.id}
                      emptyLabel={isDefault ? '未加入自定义包' : '仅当前包'}
                    />
                    {!member.missing && <span className="sm2__pack-member-open-arrow" aria-hidden="true">查看 ›</span>}
                  </button>
                  <div className="sm2__pack-member-state">
                    <span className={`sm2__tag sm2__tag--${member.missing ? 'conflict' : 'ok'}`}>
                      {member.missing ? '缺失' : '就绪'}
                    </span>
                    {!isDefault && (
                      <button className="sm2__btn sm2__btn--ghost" disabled={busy} onClick={() => onRemoveSkill(member.skillId)}>
                        移除
                      </button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
          </section>
        ) : (
          <section className="sm2__pack-section sm2__pack-section--agents">
          <div className="sm2__pack-section-head">
            <div>
              <h3>已应用 Agent</h3>
              <span>每个 Agent 独立同步；撤销只移除这个技能包带来的内容。</span>
            </div>
            <button className="sm2__btn sm2__btn--primary" onClick={onApply} disabled={busy || detail.members.length === 0 || missing.length > 0}>
              ＋ 应用到 Agent
            </button>
          </div>
          {detail.appliedAgents.length === 0 ? (
            <div className="sm2__pack-panel-empty">
              <PackEmblem compact />
              <strong>还没有应用到 Agent</strong>
              <span>选择一个或多个 Agent，这组 Skills 会一起安装并保持同步。</span>
              <button className="sm2__btn sm2__btn--primary" onClick={onApply} disabled={busy || detail.members.length === 0 || missing.length > 0}>
                选择 Agent
              </button>
            </div>
          ) : (
            <div className="sm2__pack-agent-list">
              {detail.appliedAgents.map((agent, index) => (
                <div key={agent.agentId || index} className="sm2__pack-agent-row">
                  <AgentIconBadge iconKey={agent.iconKey || agent.agentId || agent.packName} title={agent.displayName || agent.agentId || agent.packName} size={30} />
                  <div>
                    <strong>{agent.displayName || agent.agentId || agent.packName}</strong>
                    <span>{agent.memberCount} 个成员 · {packSyncStatusLabel(agent.syncStatus || 'synced')}</span>
                    {agent.syncError && <span>{agent.syncError}</span>}
                  </div>
                  {agent.agentId && agent.syncStatus && agent.syncStatus !== 'synced' && (
                    <button className="sm2__btn" disabled={busy} onClick={() => onSync([agent.agentId!])}>
                      同步
                    </button>
                  )}
                  {agent.agentId && (
                    <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => onRevoke(agent.agentId!)}>
                      撤销
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
          </section>
        )}
      </div>
    </div>
  )
}

function packNeedsSync(pack: Pick<SkillPackDetail, 'pendingSyncCount' | 'failedSyncCount' | 'syncStatus'>) {
  return (pack.pendingSyncCount || 0) + (pack.failedSyncCount || 0) > 0 || ['pending', 'failed', 'partial', 'syncing'].includes(pack.syncStatus || '')
}

function packSyncStatusLabel(status: string) {
  if (status === 'pending') return '有变更未同步'
  if (status === 'syncing') return '同步中'
  if (status === 'failed') return '同步失败'
  if (status === 'partial') return '部分同步'
  return '已同步'
}

function PackBuilderPanel({
  mode,
  runtime,
  existing,
  membershipsBySkillId,
  membershipsLoading,
  onCancel,
  onSaved,
}: {
  mode: BuilderMode
  runtime: PackRuntime
  existing: SkillPackDetail | null
  membershipsBySkillId: SkillPackMembershipsBySkillId
  membershipsLoading: boolean
  onCancel: () => void
  onSaved: (pack: SkillPackDetail, options: PackBuilderSaveOptions) => void
}) {
  const skills = useSkillStoreV2((s) => s.skills)
  const { t } = useTranslation()
  const [name, setName] = useState(mode === 'duplicate' && existing ? `${existing.name} Copy` : existing?.name || '')
  const [query, setQuery] = useState('')
  const [sourceFilter, setSourceFilter] = useState('')
  const [selected, setSelected] = useState<Set<string>>(new Set(existing?.members.map((member) => member.skillId) || []))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const existingMemberIds = useMemo(() => new Set(existing?.members.map((member) => member.skillId) || []), [existing])
  const sourceOptions = useMemo(() => {
    const counts = new Map<string, number>()
    skills.forEach((skill) => counts.set(skill.sourceType, (counts.get(skill.sourceType) || 0) + 1))
    return Array.from(counts.entries())
      .map(([value, count]) => ({
        value,
        count,
        label: skillSourceTypeLabel(t, value),
      }))
      .sort((a, b) => a.label.localeCompare(b.label))
  }, [skills, t])
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    return skills.filter((skill) =>
      (!sourceFilter || skill.sourceType === sourceFilter) &&
      (!q ||
        [skill.name, skill.id, skill.description, skill.sourceType, skillSourceTypeLabel(t, skill.sourceType)]
          .join(' ')
          .toLowerCase()
          .includes(q)),
    )
  }, [query, skills, sourceFilter, t])
  const selectedSkills = skills.filter((skill) => selected.has(skill.id))
  const selectedVisibleCount = filtered.filter((skill) => selected.has(skill.id)).length
  const editingPackId = mode === 'edit' ? existing?.id : undefined
  const emptyMembershipLabel = editingPackId ? '仅当前包' : '未加入技能包'

  const toggle = (id: string) => {
    setError(null)
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const selectVisible = () => {
    setError(null)
    setSelected((prev) => {
      const next = new Set(prev)
      filtered.forEach((skill) => next.add(skill.id))
      return next
    })
  }

  const clearSelected = () => {
    setError(null)
    setSelected(new Set())
  }

  const save = async () => {
    if (!isPackRuntimeCurrent(runtime)) return
    if (!name.trim()) {
      setError('技能包名称必填。')
      return
    }
    const skillIds = Array.from(selected)
    const removedExistingMember = mode === 'edit' && Array.from(existingMemberIds).some((id) => !selected.has(id))
    setBusy(true)
    setError(null)
    try {
      const saved = await skillApiV2.upsertPack(
        {
          id: mode === 'edit' ? existing?.id || '' : '',
          name: name.trim(),
          description: existing?.description ?? '',
          tags: existing?.tags ?? [],
          skillIds,
        },
        { deferSync: true, environmentId: runtime.selectedEnvironmentId },
      )
      if (isPackRuntimeCurrent(runtime)) onSaved(saved, { forceSync: removedExistingMember })
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="sm2__pack-builder2">
      <header className="sm2__pack-builder-head">
        <div>
          <div className="sm2__pack-kicker">{mode === 'edit' ? '编辑组合' : mode === 'duplicate' ? '复制组合' : '新建组合'}</div>
          <h3>{mode === 'edit' ? '编辑技能包' : mode === 'duplicate' ? '复制技能包' : '创建技能包'}</h3>
          <p>为组合命名，再从中心库挑选需要一起使用的 Skills。</p>
        </div>
        <button className="sm2__pack-builder-close" type="button" aria-label="关闭" onClick={onCancel} disabled={busy}>×</button>
      </header>

      <div className="sm2__pack-builder-identity">
        <label className="sm2__field">
          <span>技能包名称</span>
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="例如：Code Review 工具集" />
        </label>
        <div className="sm2__pack-builder-count" aria-live="polite">
          <strong>{selectedSkills.length}</strong>
          <span>个 Skills 已加入</span>
        </div>
      </div>

      <div className="sm2__pack-builder2-grid">
        <section className="sm2__pack-section sm2__pack-builder-card sm2__pack-section--picker">
          <div className="sm2__pack-section-head">
            <div>
              <h3>从中心库添加</h3>
              <span>{filtered.length} 个结果，点击左侧 ＋ 加入技能包。</span>
            </div>
            <span>{selectedVisibleCount} 已选</span>
          </div>
          <div className="sm2__pack-picker-tools">
            <input className="sm2__search sm2__search--full" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="按名称、描述或 ID 搜索" />
            <button className="sm2__btn sm2__btn--ghost" onClick={selectVisible} disabled={filtered.length === 0 || selectedVisibleCount === filtered.length}>
              全选当前
            </button>
          </div>
          <div className="sm2__pack-source-filter" aria-label="来源过滤">
            <span>来源过滤</span>
            <button
              type="button"
              className={!sourceFilter ? 'sm2__pack-source-filter-chip sm2__pack-source-filter-chip--active' : 'sm2__pack-source-filter-chip'}
              aria-pressed={!sourceFilter}
              onClick={() => setSourceFilter('')}
            >
              全部 <em>{skills.length}</em>
            </button>
            {sourceOptions.map((source) => (
              <button
                key={source.value}
                type="button"
                className={sourceFilter === source.value ? 'sm2__pack-source-filter-chip sm2__pack-source-filter-chip--active' : 'sm2__pack-source-filter-chip'}
                aria-pressed={sourceFilter === source.value}
                onClick={() => setSourceFilter(source.value)}
              >
                {source.label} <em>{source.count}</em>
              </button>
            ))}
          </div>
          <div className="sm2__skill-picker2">
            {filtered.length === 0 ? (
              <div className="sm2__empty sm2__empty--compact">没有匹配的 Skill</div>
            ) : (
              filtered.map((skill) => (
                <div
                  key={skill.id}
                  className={`sm2__skill-pick${selected.has(skill.id) ? ' sm2__skill-pick--selected' : ''}`}
                >
                  <button
                    type="button"
                    className="sm2__skill-pick-action"
                    aria-label={selected.has(skill.id) ? `从技能包移除 ${skill.name}` : `添加 ${skill.name} 到技能包`}
                    aria-pressed={selected.has(skill.id)}
                    onClick={() => toggle(skill.id)}
                  >
                    <span aria-hidden="true">{selected.has(skill.id) ? '✓' : '+'}</span>
                  </button>
                  <span className="sm2__skill-pick-body">
                    <strong>{skill.name}</strong>
                    <span>{skill.description || skill.id}</span>
                  </span>
                  <SkillPackMembershipTags
                    packs={membershipsBySkillId[skill.id] || []}
                    loading={membershipsLoading}
                    excludePackId={editingPackId}
                    emptyLabel={emptyMembershipLabel}
                  />
                  <span className="sm2__tag">{skillSourceTypeLabel(t, skill.sourceType)}</span>
                </div>
              ))
            )}
          </div>
        </section>

        <section className="sm2__pack-section sm2__pack-builder-card sm2__pack-builder-card--selected">
          <div className="sm2__pack-section-head">
            <div>
              <h3>包内 Skills</h3>
              <span>保存后，这些成员会作为一个组合使用。</span>
            </div>
            <span role="status" aria-label="已选择 Skill 数量">{selectedSkills.length}</span>
          </div>
          <div className="sm2__pack-selected-tools">
            <span>{selectedSkills.length === 0 ? '从左侧选择 Skill' : `已加入 ${selectedSkills.length} 个 Skill`}</span>
            <button className="sm2__btn sm2__btn--ghost" onClick={clearSelected} disabled={selectedSkills.length === 0}>
              清空
            </button>
          </div>
          {selectedSkills.length === 0 ? (
            <div className="sm2__pack-selected-empty">
              <PackEmblem compact />
              <strong>还没有成员</strong>
              <span>点击左侧 ＋，Skill 会立即加入这个包。</span>
            </div>
          ) : (
            <div className="sm2__pack-member-list">
              {selectedSkills.map((skill) => (
                <div key={skill.id} className="sm2__pack-member-row">
                  <div className="sm2__pack-selected-marker" aria-hidden="true">✓</div>
                  <div className="sm2__pack-member-body">
                    <strong>{skill.name}</strong>
                    <span>{skill.id}</span>
                  </div>
                  <SkillPackMembershipTags
                    packs={membershipsBySkillId[skill.id] || []}
                    loading={membershipsLoading}
                    excludePackId={editingPackId}
                    emptyLabel={emptyMembershipLabel}
                  />
                  <button className="sm2__btn sm2__btn--ghost" aria-label={`移除 ${skill.name}`} onClick={() => toggle(skill.id)}>移除</button>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
      <footer className="sm2__pack-builder-footer">
        <span>
          {name.trim()
            ? `“${name.trim()}”将保存 ${selectedSkills.length} 个 Skill`
            : '填写名称后即可保存技能包'}
        </span>
        <div className="sm2__pack-actions">
          <button className="sm2__btn" onClick={onCancel} disabled={busy}>取消</button>
          <button className="sm2__btn sm2__btn--primary" onClick={save} disabled={busy || !name.trim()}>
            {busy ? '保存中…' : mode === 'edit' ? '保存更改' : '创建技能包'}
          </button>
        </div>
      </footer>
    </div>
  )
}

function SkillPackMembershipTags({
  packs,
  loading,
  excludePackId,
  emptyLabel,
}: {
  packs: SkillPackSummary[]
  loading: boolean
  excludePackId?: string
  emptyLabel: string
}) {
  const visiblePacks = packs.filter((pack) => pack.id !== excludePackId)
  const isOverlap = Boolean(excludePackId)
  const ariaLabel = loading
    ? '正在载入技能包归属'
    : visiblePacks.length > 0
      ? `${isOverlap ? '还属于' : '所属'}技能包：${visiblePacks.map((pack) => pack.name).join('、')}`
      : emptyLabel

  return (
    <span
      className={`sm2__pack-memberships${visiblePacks.length === 0 ? ' sm2__pack-memberships--empty' : ''}`}
      aria-label={ariaLabel}
      title={ariaLabel}
    >
      <span className="sm2__pack-memberships-label">{isOverlap ? '还在' : '技能包'}</span>
      {visiblePacks.length > 0 ? visiblePacks.map((pack) => (
        <span key={pack.id} className="sm2__pack-membership-chip" title={pack.name}>{pack.name}</span>
      )) : (
        <span className="sm2__pack-membership-empty">{loading ? '归属载入中…' : emptyLabel}</span>
      )}
    </span>
  )
}

function DeletePackDialog({
  preview,
  busy,
  onClose,
  onDelete,
}: {
  preview: DeleteSkillPackPreview
  busy: boolean
  onClose: () => void
  onDelete: () => void
}) {
  return (
    <PreviewDialog
      title={`删除技能包「${preview.packName}」`}
      confirmLabel="删除"
      destructive
      busy={busy}
      disabled={!preview.removable}
      onConfirm={onDelete}
      onCancel={onClose}
    >
      {preview.warnings.map((warning, index) => (
        <div key={index} className="sm2__change sm2__change--blocked">{warning}</div>
      ))}
      {preview.appliedAgents.length > 0 && (
        <div className="sm2__detail-meta">已应用 Agent：{preview.appliedAgents.join('、')}</div>
      )}
      {preview.affectedTargets.length > 0 && (
        <div className="sm2__scroll" style={{ marginTop: 8 }}>
          {preview.affectedTargets.map((target) => (
            <div key={target.targetId} className="sm2__target-row">
              <span>{target.displayName}</span>
              <code>{target.targetPath}</code>
            </div>
          ))}
        </div>
      )}
    </PreviewDialog>
  )
}

function RemoveSkillFromPackDialog({
  preview,
  busy,
  onClose,
  onExecute,
}: {
  preview: RemoveSkillFromPackPreview
  busy: boolean
  onClose: () => void
  onExecute: (alsoRemoveTargets: boolean) => void
}) {
  return (
    <PreviewDialog
      title={`从「${preview.packName}」移除「${preview.skillName}」`}
      confirmLabel="保留为直接分发"
      busy={busy}
      onConfirm={() => onExecute(false)}
      onCancel={onClose}
    >
      <div className="sm2__detail-meta">
        影响 {preview.appliedAgentCount} 个 Agent，{preview.affectedTargets.length} 个安装目标。
      </div>
      {preview.affectedTargets.length > 0 && (
        <div className="sm2__scroll" style={{ marginTop: 8 }}>
          {preview.affectedTargets.map((target) => (
            <div key={target.targetId} className="sm2__target-row">
              <span>{target.displayName} · {target.mode} · {target.claimCount} claim(s)</span>
              <code>{target.targetPath}</code>
            </div>
          ))}
        </div>
      )}
      <div className="sm2__btn-row">
        <button className="sm2__btn sm2__btn--danger" disabled={busy} onClick={() => onExecute(true)}>
          同步从 Agent 移除
        </button>
      </div>
    </PreviewDialog>
  )
}

function RevokePackDialog({
  preview,
  busy,
  onClose,
  onExecute,
}: {
  preview: RemovePackFromAgentPreview
  busy: boolean
  onClose: () => void
  onExecute: () => void
}) {
  return (
    <PreviewDialog
      title={`从 ${preview.displayName} 撤销「${preview.packName}」`}
      confirmLabel="撤销技能包"
      destructive
      busy={busy}
      onConfirm={onExecute}
      onCancel={onClose}
    >
      <div className="sm2__detail-meta">
        将移除 {preview.willRemoveTargets} 个仅由该技能包安装的目标，保留 {preview.willPreserveTargets} 个仍有其他 claim 的目标。
      </div>
      {preview.affectedTargets.length > 0 && (
        <div className="sm2__scroll" style={{ marginTop: 8 }}>
          {preview.affectedTargets.map((target) => (
            <div key={target.targetId} className="sm2__target-row">
              <span>{target.displayName} · {target.mode} · {target.claimCount} claim(s)</span>
              <code>{target.targetPath}</code>
            </div>
          ))}
        </div>
      )}
    </PreviewDialog>
  )
}

function PackSyncConflictDialog({
  conflict,
  onClose,
  onDone,
}: {
  conflict: PackSyncConflictState
  onClose: () => void
  onDone: () => void
}) {
  const { t } = useTranslation()
  const [preview, setPreview] = useState(conflict.preview)
  const [blockerDecisions, setBlockerDecisions] = useState<Record<string, BlockerDecision>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const changesByAction = useMemo(() => {
    const grouped = { create: 0, reuse: 0, other: 0 }
    for (const change of preview.changes) {
      if (change.action === 'create') grouped.create += 1
      else if (change.action === 'reuse') grouped.reuse += 1
      else grouped.other += 1
    }
    return grouped
  }, [preview])
  const unresolvedBlockers = preview.blockers.filter((blocker) => !blockerDecisions[blockerKey(blocker)]).length

  const execute = async () => {
    const isCurrent = () => isPackRuntimeCurrent(conflict.runtime)
      && useSkillStoreV2.getState().selectedPackId === conflict.packId
    if (!isCurrent()) return
    setBusy(true)
    setError(null)
    try {
      const blockerDecisionsPayload = preview.blockers
        .map((blocker) => {
          const action = blockerDecisions[blockerKey(blocker)]
          return action ? { skillId: blocker.skillId, agentId: blocker.agentId, action } : null
        })
        .filter((item): item is DistributionBlockerDecision => Boolean(item))
      const result = await skillApiV2.executeApplyPack(
        conflict.packId,
        conflict.targetAgents,
        conflict.requestedMode,
        blockerDecisionsPayload,
        preview,
        conflict.runtime.selectedEnvironmentId,
      )
      if (!isCurrent()) return
      if (result.blockers.length > 0) {
        setPreview(result)
        setBlockerDecisions({})
        return
      }
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <PreviewDialog
      title="确认同步冲突"
      confirmLabel="按选择同步"
      destructive
      busy={busy}
      disabled={unresolvedBlockers > 0}
      onConfirm={execute}
      onCancel={onClose}
    >
      <div className="sm2__notice sm2__notice--warn" style={{ margin: 0 }}>
        当前同步的 Agent 里面有未接管的同名 Skill。请选择是否覆盖安装，或忽略该目标。
      </div>
      <div className="sm2__preview-stats">
        <PackMetric label="新增 target" value={changesByAction.create} />
        <PackMetric label="复用 target" value={changesByAction.reuse} />
        <PackMetric label="阻止项" value={preview.blockers.length} tone={preview.blockers.length > 0 ? 'warn' : 'ok'} />
      </div>
      {preview.changes.map((change, index) => (
        <div key={index} className="sm2__change">
          {change.action === 'create'
            ? `新增 ${change.agentId} / ${change.skillId} (${skillModeLabel(t, change.actualMode)})`
            : `复用 ${change.agentId} / ${change.skillId}`}
        </div>
      ))}
      {preview.blockers.map((blocker, index) => (
        <div key={index} className="sm2__change sm2__change--blocked">
          <div>
            阻止 {blocker.skillId}/{blocker.agentId}：{distributionBlockerMessage(blocker)}
          </div>
          {blocker.existingPath && (
            <div className="sm2-distribute__path-row" style={{ marginTop: 8 }}>
              <code>{blocker.existingPath}</code>
              <button type="button" className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(blocker.existingPath!)}>
                打开
              </button>
            </div>
          )}
          <div className="sm2-distribute__decision-row" role="radiogroup" aria-label={`${blocker.agentId} 同步冲突处理方式`}>
            {blocker.existingPath && (
              <button
                type="button"
                className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'overwrite' ? ' sm2-distribute__decision--active' : ''}`}
                onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'overwrite' }))}
              >
                {isManagedCopyBlocker(blocker) ? '以中心库为准' : '覆盖安装'}
              </button>
            )}
            {isManagedCopyBlocker(blocker) && (
              <button
                type="button"
                className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'agent_over_center' ? ' sm2-distribute__decision--active' : ''}`}
                onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'agent_over_center' }))}
              >
                以 Agent 为准
              </button>
            )}
            <button
              type="button"
              className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'skip' ? ' sm2-distribute__decision--active' : ''}`}
              onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'skip' }))}
            >
              忽略此目标
            </button>
          </div>
        </div>
      ))}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}

function distributionBlockerMessage(blocker: ConflictBlocker) {
  if (blocker.reason.startsWith('An unmanaged ')) {
    return '目标路径已存在未接管的同名 Skill，需要先选择覆盖安装或忽略。'
  }
  if (isManagedCopyBlocker(blocker)) {
    return '已管理的 copy 版本有本地修改，需要选择中心库或 Agent 版本为准。'
  }
  return blocker.reason
}

function isPackRuntimeCurrent(runtime: PackRuntime) {
  const current = useRuntimeEnvironmentStore.getState()
  return current.selectedEnvironmentId === runtime.selectedEnvironmentId
    && current.selectionRevision === runtime.selectionRevision
    && useSkillStoreV2.getState().runtimeEnvironmentId === runtime.selectedEnvironmentId
}

function ApplyPackDialog({
  pack,
  runtime,
  suggestedAgentIds,
  agents,
  defaultMode,
  onClose,
  onDone,
}: {
  pack: SkillPackDetail
  runtime: PackRuntime
  suggestedAgentIds: string[]
  agents: AgentSummary[]
  defaultMode: 'link' | 'copy'
  onClose: () => void
  onDone: () => void | Promise<void>
}) {
  const { t } = useTranslation()
  const [selection, setSelection] = useState<Set<string> | null>(null)
  const [mode, setMode] = useState<'link' | 'copy'>(defaultMode)
  const [preview, setPreview] = useState<DistributionPreview | null>(null)
  const [blockerDecisions, setBlockerDecisions] = useState<Record<string, BlockerDecision>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const previewRequest = useRef(0)
  const installedAgents = agents.filter((agent) => agent.installed && agent.id !== SHARED_SKILLS_AGENT_ID)
  const suggestions = new Set(installedAgents.filter((agent) =>
    agent.isCustom === false && suggestedAgentIds.includes(agent.id),
  ).map((agent) => agent.id))
  const unavailable = suggestedAgentIds.filter((id) => !suggestions.has(id))
  const selected = selection ?? suggestions
  const targetAgents = Array.from(selected).filter((id) => installedAgents.some((agent) => agent.id === id))
  const isCurrentApplication = () => isPackRuntimeCurrent(runtime)
    && useSkillStoreV2.getState().selectedPackId === pack.id
  const changesByAction = useMemo(() => {
    const grouped = { create: 0, reuse: 0, other: 0 }
    if (!preview) return grouped
    for (const change of preview.changes) {
      if (change.action === 'create') grouped.create += 1
      else if (change.action === 'reuse') grouped.reuse += 1
      else grouped.other += 1
    }
    return grouped
  }, [preview])

  const resetPreview = () => {
    previewRequest.current += 1
    setPreview(null)
    setBlockerDecisions({})
    setBusy(false)
    setError(null)
  }

  const toggle = (id: string) => {
    resetPreview()
    setSelection((prev) => {
      const next = new Set(prev ?? suggestions)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const runPreview = async () => {
    if (busy || !targetAgents.length || !isCurrentApplication()) return
    const requestId = ++previewRequest.current
    const isCurrent = () => requestId === previewRequest.current && isCurrentApplication()
    setBusy(true)
    setError(null)
    try {
      const result = await skillApiV2.previewApplyPack(pack.id, targetAgents, mode, runtime.selectedEnvironmentId)
      if (isCurrent()) setPreview(result)
    } catch (e) {
      if (isCurrent()) setError(String(e))
    } finally {
      if (requestId === previewRequest.current) setBusy(false)
    }
  }

  const execute = async () => {
    if (!preview || busy || !isCurrentApplication()) return
    setBusy(true)
    setError(null)
    try {
      const blockerDecisionsPayload = preview.blockers
        .map((blocker) => {
          const action = blockerDecisions[blockerKey(blocker)]
          return action ? { skillId: blocker.skillId, agentId: blocker.agentId, action } : null
        })
        .filter((item): item is DistributionBlockerDecision => Boolean(item))
      const result = await skillApiV2.executeApplyPack(
        pack.id, preview.targetAgents, preview.requestedMode, blockerDecisionsPayload, preview, runtime.selectedEnvironmentId,
      )
      if (!isCurrentApplication()) return
      if (result.blockers.length > 0) {
        setPreview(result)
        setBlockerDecisions({})
        return
      }
      await onDone()
    } catch (e) {
      if (isCurrentApplication()) setError(String(e))
    } finally {
      setBusy(false)
    }
  }

  if (!preview) {
    return (
      <PreviewDialog
        title={`应用「${pack.name}」`}
        confirmLabel="预览影响"
        modalClassName="sm2__modal--pack-apply"
        busy={busy}
        disabled={targetAgents.length === 0}
        onConfirm={runPreview}
        onCancel={onClose}
      >
        <div className="sm2__pack-apply-form">
          <div className="sm2__apply-grid">
            <div className="sm2__field sm2__field--select">
              <label htmlFor="sm2-pack-apply-mode">分发方式</label>
              <select id="sm2-pack-apply-mode" value={mode} onChange={(event) => {
                resetPreview()
                setMode(event.target.value as 'link' | 'copy')
              }}>
                <option value="link">{skillModeLabel(t, 'link')}</option>
                <option value="copy">{skillModeLabel(t, 'copy')}</option>
              </select>
            </div>
            <div className="sm2__field sm2__field--readonly">
              <label htmlFor="sm2-pack-apply-members">成员数量</label>
              <input id="sm2-pack-apply-members" value={`${pack.members.length} Skills`} readOnly />
            </div>
          </div>
          <div className="sm2__pack-apply-section">
            <div className="sm2__pack-apply-label">目标 Agent</div>
            <div className="sm2__agent-choice-grid">
              {installedAgents.map((agent) => (
                <button
                  key={agent.id}
                  type="button"
                  aria-pressed={selected.has(agent.id)}
                  className={`sm2__agent-choice${selected.has(agent.id) ? ' sm2__agent-choice--active' : ''}`}
                  onClick={() => toggle(agent.id)}
                >
                  <AgentIconBadge iconKey={agent.iconKey} title={agent.displayName} size={30} />
                  <span>{agent.displayName}</span>
                </button>
              ))}
            </div>
          </div>
          {unavailable.length > 0 && (
            <p className="sm2__notice sm2__notice--warn">{t('skills.packManifest.unavailableAgents', {
              agents: suggestedAgentNames(unavailable, agents),
            })}</p>
          )}
          {installedAgents.length === 0 && <div className="sm2__empty sm2__empty--compact">没有可用 Agent</div>}
          {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
        </div>
      </PreviewDialog>
    )
  }

  const unresolvedBlockers = preview.blockers.filter((blocker) => !blockerDecisions[blockerKey(blocker)]).length

  return (
    <PreviewDialog
      title="确认应用预览"
      confirmLabel={preview.blockers.length > 0 ? '按选择执行' : '执行应用'}
      destructive
      busy={busy}
      disabled={unresolvedBlockers > 0}
      onConfirm={execute}
      onCancel={() => { if (!busy) resetPreview() }}
    >
      <div className="sm2__detail-meta">
        {preview.targetAgents.map((id) => agents.find((agent) => agent.id === id)?.displayName ?? id).join('、')}
        {' · '}{skillModeLabel(t, preview.requestedMode)}
      </div>
      <div className="sm2__preview-stats">
        <PackMetric label="新增 target" value={changesByAction.create} />
        <PackMetric label="复用 target" value={changesByAction.reuse} />
        <PackMetric label="阻止项" value={preview.blockers.length} tone={preview.blockers.length > 0 ? 'warn' : 'ok'} />
      </div>
      {preview.changes.map((change, index) => (
        <div key={index} className="sm2__change">
          {change.action === 'create'
            ? `新增 ${change.agentId} / ${change.skillId} (${skillModeLabel(t, change.actualMode)})`
            : `复用 ${change.agentId} / ${change.skillId}`}
        </div>
      ))}
      {preview.blockers.map((blocker, index) => (
        <div key={index} className="sm2__change sm2__change--blocked">
          <div>
            阻止 {blocker.skillId}/{blocker.agentId}：{distributionBlockerMessage(blocker)}
          </div>
          {blocker.existingPath && (
            <div className="sm2-distribute__path-row" style={{ marginTop: 8 }}>
              <code>{blocker.existingPath}</code>
              <button type="button" className="sm2__btn sm2__btn--ghost" onClick={() => skillApiV2.openPath(blocker.existingPath!)}>
                打开
              </button>
            </div>
          )}
          <div className="sm2-distribute__decision-row" role="radiogroup" aria-label={`${blocker.agentId} 阻止项处理方式`}>
            {blocker.existingPath && (
              <button
                type="button"
                className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'overwrite' ? ' sm2-distribute__decision--active' : ''}`}
                onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'overwrite' }))}
              >
                {isManagedCopyBlocker(blocker) ? '以中心库为准' : '覆盖安装'}
              </button>
            )}
            {isManagedCopyBlocker(blocker) && (
              <button
                type="button"
                className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'agent_over_center' ? ' sm2-distribute__decision--active' : ''}`}
                onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'agent_over_center' }))}
              >
                以 Agent 为准
              </button>
            )}
            <button
              type="button"
              className={`sm2-distribute__decision${blockerDecisions[blockerKey(blocker)] === 'skip' ? ' sm2-distribute__decision--active' : ''}`}
              onClick={() => setBlockerDecisions((prev) => ({ ...prev, [blockerKey(blocker)]: 'skip' }))}
            >
              忽略此目标
            </button>
          </div>
        </div>
      ))}
      {error && <div className="sm2__error" style={{ margin: 0 }}>{error}</div>}
    </PreviewDialog>
  )
}
