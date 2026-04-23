// @ts-check
import withNuxt from './.nuxt/eslint.config.mjs'
import prettier from 'eslint-config-prettier'

export default withNuxt(
  // Disable ESLint rules that collide with Prettier's formatting.
  prettier
)
