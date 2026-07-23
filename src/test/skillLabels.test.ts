import { afterEach, describe, expect, it } from 'vitest'
import i18n from '../i18n'
import { skillErrorMessage } from '../components/skills-v2/skillLabels'

const unavailableAdoptError = new Error(
  "Adopt option 'import_keep' is not allowed for 'fsd-alipay-business-skill'. Re-run preview and choose one of the suggested actions.",
)

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
})
