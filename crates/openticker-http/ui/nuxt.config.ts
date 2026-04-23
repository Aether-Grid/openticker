// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  modules: ['@nuxt/eslint', '@nuxt/ui', '@vueuse/nuxt'],

  ssr: false,

  devtools: {
    enabled: true
  },

  css: ['~/assets/css/main.css'],

  compatibilityDate: '2025-01-15',

  runtimeConfig: {
    public: {
      apiBase: ''
    }
  },

  routeRules: {
    '/': { prerender: true }
  },

  nitro: {
    prerender: {
      routes: ['/', '/200.html']
    },
    devProxy: {
      '/v1': { target: 'http://127.0.0.1:8080/v1', changeOrigin: true },
      '/api': { target: 'http://127.0.0.1:8080/api', changeOrigin: true },
      '/openapi.json': { target: 'http://127.0.0.1:8080/openapi.json', changeOrigin: true },
      '/healthz': { target: 'http://127.0.0.1:8080/healthz', changeOrigin: true },
      '/readyz': { target: 'http://127.0.0.1:8080/readyz', changeOrigin: true },
      '/metrics': { target: 'http://127.0.0.1:8080/metrics', changeOrigin: true }
    }
  },

  app: {
    head: {
      title: 'OpenTicker — Control Workspace',
      meta: [
        { name: 'viewport', content: 'width=device-width, initial-scale=1' },
        {
          name: 'description',
          content: 'Local control plane for algorithmic trading bots. Bots, cycles, portfolio, and live market data.'
        }
      ],
      link: [
        { rel: 'icon', href: '/favicon.ico' },
        { rel: 'preconnect', href: 'https://fonts.googleapis.com' },
        { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' },
        {
          rel: 'stylesheet',
          href: 'https://fonts.googleapis.com/css2?family=Geist:wght@300;400;450;500;600;700&family=Instrument+Serif:ital@0;1&family=JetBrains+Mono:wght@400;500;600&display=swap'
        }
      ],
      htmlAttrs: {
        lang: 'en'
      }
    }
  },

  colorMode: {
    preference: 'light',
    fallback: 'light'
  },

  eslint: {
    config: {
      // Formatting is handled by Prettier (.prettierrc.json).
      stylistic: false
    }
  }
})
