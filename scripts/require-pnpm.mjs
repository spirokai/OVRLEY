if (!process.env.npm_config_user_agent?.startsWith('pnpm/')) {
  console.error('OVRLEY uses pnpm. Run "pnpm install" from the repository root; package.json pins the required version.')
  process.exit(1)
}
