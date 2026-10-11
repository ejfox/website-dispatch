// One name per thing. Labels that show up in more than one place live here so
// the tab, the banner, the list pill and the palette can't drift apart.
// User words come first; git/frontmatter words only appear as secondary text.

/** A live post whose vault copy no longer matches the site. (Internally:
 *  the `Modified since publish` warning. That string is a key; don't show it.) */
export const CHANGED_LABEL = 'Changed since live'
// The list pill is short so titles keep their room; its tooltip says the rest.
export const CHANGED_PILL = 'CHANGED'
export const CHANGED_HINT = 'Your vault copy differs from what’s on ejfox.com'

/** A vault post that isn't on the site. Not the same as `draft: true`
 *  (which is shown literally) or the repo's content/drafts/ folder. */
export const NOT_PUBLISHED_LABEL = 'Not published'

/** The real-renderer window on 127.0.0.1:6419, as opposed to the Preview tab. */
export const SITE_PREVIEW_LABEL = 'Open site preview'
export const SITE_PREVIEW_HINT = 'Renders with ejfox.com’s actual pipeline, in a separate window'
