export default defineAppConfig({
  ui: {
    colors: {
      primary: 'indigo',
      neutral: 'stone'
    },
    button: {
      slots: {
        base: 'rounded-md font-medium tracking-tight',
        leadingIcon: 'shrink-0'
      }
    },
    card: {
      slots: {
        root: 'rounded-xl ring-0 border border-[color:var(--ui-border-muted)] bg-[color:var(--ui-bg)] shadow-none',
        header: 'border-b border-[color:var(--ui-border-muted)] px-6 py-4',
        body: 'px-6 py-5',
        footer: 'border-t border-[color:var(--ui-border-muted)] px-6 py-4'
      }
    },
    badge: {
      slots: {
        base: 'rounded-full font-medium tracking-[0.04em] uppercase'
      }
    }
  }
})
