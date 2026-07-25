import { afterEach, describe, expect, it } from 'vitest'
import i18n from '../i18n'
import { distributionBlockerReason, skillErrorMessage } from '../components/skills-v2/skillLabels'

const unavailableAdoptError = new Error(
  "Adopt option 'import_keep' is not allowed for 'fsd-alipay-business-skill'. Re-run preview and choose one of the suggested actions.",
)
const unmanagedAgentMismatchError = new Error(
  "Unmanaged item 'unm-agents-Users-me--agents-skills-bird' does not belong to agent 'codex'.",
)
const unmanagedDistributionBlocker = {
  skillId: 'lark-approval',
  agentId: 'zcode',
  reason: "An unmanaged 'lark-approval' already exists at the target path. Adopt/overwrite/rename it first.",
  existingPath: '/Users/me/.zcode/skills/lark-approval',
}

describe('skill error labels', () => {
  afterEach(async () => {
    await i18n.changeLanguage('zh')
  })

  it.each([
    ['zh', '当前选择的接管方式已不可用。请重新打开接管预览，并从建议操作中选择。'],
    ['en', 'The selected adoption method is no longer available. Reopen the adoption preview and choose one of the suggested actions.'],
    ['ja', '選択した引き継ぎ方法は現在利用できません。引き継ぎプレビューを開き直し、提案された操作から選択してください。'],
    ['ko', '선택한 인계 방식은 더 이상 사용할 수 없습니다. 인계 미리보기를 다시 열고 제안된 작업 중 하나를 선택하세요.'],
    ['tr', 'Seçilen yönetimi devralma yöntemi artık kullanılamıyor. Devralma önizlemesini yeniden açın ve önerilen işlemlerden birini seçin.'],
  ])('localizes unavailable adoption options in %s', async (language, expected) => {
    await i18n.changeLanguage(language)
    expect(skillErrorMessage(i18n.t, unavailableAdoptError)).toBe(expected)
  })

  it.each([
    ['zh', '该未管理 Skill 不属于 Agent「codex」，请重新扫描后重试。'],
    ['en', "This unmanaged Skill does not belong to Agent 'codex'. Rescan and try again."],
    ['ja', 'この未管理 Skill は Agent「codex」に属していません。再スキャンしてからもう一度お試しください。'],
    ['ko', "이 관리되지 않는 Skill은 Agent 'codex'에 속하지 않습니다. 다시 스캔한 뒤 재시도하세요."],
    ['tr', "Bu yönetilmeyen Skill, 'codex' Agent'ına ait değil. Yeniden tarayıp tekrar deneyin."],
  ])('localizes unmanaged Agent mismatch errors in %s', async (language, expected) => {
    await i18n.changeLanguage(language)
    expect(skillErrorMessage(i18n.t, unmanagedAgentMismatchError)).toBe(expected)
  })

  it.each([
    ['zh', '目标路径已存在未管理的 Skill「lark-approval」。请选择覆盖安装或忽略此目标。'],
    ['en', 'An unmanaged Skill “lark-approval” already exists at the target path. Choose overwrite or skip for this target.'],
    ['ja', '対象パスには未管理の Skill「lark-approval」がすでに存在します。この対象を上書きするかスキップするか選択してください。'],
    ['ko', '대상 경로에 관리되지 않는 Skill ‘lark-approval’이(가) 이미 있습니다. 이 대상을 덮어쓸지 건너뛸지 선택하세요.'],
    ['tr', "Hedef yolda yönetilmeyen “lark-approval” Skill'i zaten var. Bu hedefin üzerine yazmayı veya hedefi atlamayı seçin."],
  ])('localizes unmanaged distribution blockers in %s', async (language, expected) => {
    await i18n.changeLanguage(language)
    expect(distributionBlockerReason(i18n.t, unmanagedDistributionBlocker)).toBe(expected)
  })
})
