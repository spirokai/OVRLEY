import 'dotenv/config'
import { env } from 'node:process'
import { defineConfig } from 'i18next-cli'
import { locales } from './src/i18n/locales.js'

export default defineConfig({
  locales: locales,

  extract: {
    input: ['src/**/*.{ts, tsx, js,jsx}'],
    output: 'src/i18n/locales/{{language}}-translation.json',

    primaryLanguage: 'en',

    defaultNS: 'translation',
    nsSeparator: false,
    keySeparator: '.',

    sort: true,
    indentation: 2,
  },

  locize: {
    projectId: env.LOCIZE_PROJECTID,
    apiKey: env.LOCIZE_API_KEY,
    version: 'latest',

    autoTranslate: true,
    autoTranslateReview: false,
  },
})
