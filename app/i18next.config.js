import 'dotenv/config'
import { env } from 'node:process'
import { defineConfig } from 'i18next-cli'
import { sourceKeyInstrumentationPlugin } from './src/i18n/instrumentation.js'
import { locales } from './src/i18n/locales.js'

export default defineConfig({
  locales: locales,

  extract: {
    input: ['src/**/*.{js,jsx}', '!src/tests/**'],
    output: 'src/i18n/locales/{{language}}-translation.json',

    primaryLanguage: 'en',

    defaultNS: 'translation',
    nsSeparator: false,
    keySeparator: '.',
    conflictDefaultValues: 'error',

    sort: true,
    indentation: 2,
  },

  plugins: [sourceKeyInstrumentationPlugin],

  locize: {
    projectId: env.LOCIZE_PROJECTID,
    apiKey: env.LOCIZE_API_KEY,
    version: 'latest',

    autoTranslate: true,
    autoTranslateReview: false,
  },
})
