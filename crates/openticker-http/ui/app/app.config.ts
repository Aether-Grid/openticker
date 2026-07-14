export default defineAppConfig({
  ui: {
    colors: {
      primary: 'indigo',
      neutral: 'stone'
    },
    button: {
      slots: {
        base: 'cursor-pointer rounded-md font-medium tracking-tight disabled:cursor-not-allowed aria-disabled:cursor-not-allowed',
        leadingIcon: 'shrink-0'
      }
    },
    dropdownMenu: {
      slots: {
        item: 'cursor-pointer data-disabled:cursor-not-allowed'
      }
    },
    navigationMenu: {
      slots: {
        link: 'cursor-pointer',
        childLink: 'cursor-pointer'
      }
    },
    select: {
      slots: {
        base: 'cursor-pointer disabled:cursor-not-allowed',
        item: 'cursor-pointer data-disabled:cursor-not-allowed'
      }
    },
    selectMenu: {
      slots: {
        base: 'cursor-pointer disabled:cursor-not-allowed',
        item: 'cursor-pointer data-disabled:cursor-not-allowed'
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
