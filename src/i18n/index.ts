import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import en from './locales/en.json'
import zh from './locales/zh.json'
import ja from './locales/ja.json'
import ko from './locales/ko.json'
import tr from './locales/tr.json'

function detectLanguage(): string {
  const lang = navigator.language.toLowerCase()
  if (lang.startsWith('zh')) return 'zh'
  if (lang.startsWith('ja')) return 'ja'
  if (lang.startsWith('ko')) return 'ko'
  if (lang.startsWith('tr')) return 'tr'
  return 'en'
}

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    zh: { translation: zh },
    ja: { translation: ja },
    ko: { translation: ko },
    tr: { translation: tr },
  },
  lng: detectLanguage(),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
})

export default i18n
