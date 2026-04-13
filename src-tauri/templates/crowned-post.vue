<script setup>
/**
 * CROWNED POST: __TITLE__
 * Hue: __HUE__ | Scaffolded from Dispatch
 */

import { useIntersectionObserver } from '@vueuse/core'
import PostMetadataBar from '~/components/blog/post/PostMetadataBar.vue'
import PostNav from '~/components/blog/post/PostNav.vue'
import PostRelated from '~/components/blog/post/PostRelated.vue'
import Webmentions from '~/components/blog/Webmentions.vue'

const {
  post, nextPrevPosts, relatedPosts, readingStats,
  postTitle, postUrl, articleTags,
  renderedTitle, startAnimation,
  palette,
} = await useCrownedPost({
  slug: '__SLUG__',
  bodyClass: '__BODY_CLASS__',
  hue: __HUE__,
  fallbackTitle: '__TITLE__',
})

const articleContent = ref(null)
const scrollProgress = ref(0)

onMounted(() => {
  startAnimation()

  const handleScroll = () => {
    const scrollHeight = document.documentElement.scrollHeight - window.innerHeight
    scrollProgress.value = scrollHeight <= 0 ? 0 : Math.min((window.scrollY / scrollHeight) * 100, 100)
  }
  window.addEventListener('scroll', handleScroll)
  onUnmounted(() => window.removeEventListener('scroll', handleScroll))

  nextTick(() => {
    if (!articleContent.value) return
    const elements = articleContent.value.querySelectorAll('h2, h3, h4, p, blockquote, pre, img, figure, ul, ol')

    import('animejs').then(({ animate }) => {
      const seen = new WeakSet()
      elements.forEach((el, idx) => {
        if (el.getBoundingClientRect().top < window.innerHeight) return
        const tag = el.tagName.toLowerCase()
        const isHeading = ['h2', 'h3', 'h4'].includes(tag)
        const isMedia = ['img', 'figure'].includes(tag)
        const isQuote = tag === 'blockquote'

        el.style.opacity = '0'
        if (isHeading) el.style.transform = 'translateX(-20px)'
        else if (isMedia) { el.style.transform = 'scale(0.97)'; el.style.filter = 'blur(4px)' }
        else if (isQuote) el.style.transform = 'translateX(20px)'
        else el.style.transform = 'translateY(12px)'

        const obs = new IntersectionObserver(([entry]) => {
          if (!entry.isIntersecting || seen.has(el)) return
          seen.add(el)
          obs.disconnect()
          const cleanup = () => { el.style.opacity = ''; el.style.transform = ''; el.style.filter = '' }

          if (isHeading) animate(el, { opacity: [0, 1], translateX: [-20, 0], duration: 500, ease: 'outCubic', onComplete: cleanup })
          else if (isMedia) animate(el, { opacity: [0, 1], scale: [0.97, 1], filter: ['blur(4px)', 'blur(0px)'], duration: 600, ease: 'outQuad', onComplete: cleanup })
          else if (isQuote) animate(el, { opacity: [0, 1], translateX: [20, 0], duration: 500, ease: 'outCubic', onComplete: cleanup })
          else animate(el, { opacity: [0, 1], translateY: [12, 0], duration: 400, delay: Math.min((idx % 3) * 60, 120), ease: 'outQuad', onComplete: cleanup })
        }, { threshold: 0.05, rootMargin: '0px 0px -10% 0px' })
        obs.observe(el)
      })
    })
  })
})
</script>

<template>
  <div class="__CSS_CLASS__">
    <div class="crowned-progress">
      <div class="crowned-progress-inner" :style="{ width: scrollProgress + '%' }" />
    </div>

    <div v-if="post" class="fixed top-0 left-0 right-0 z-[100] bg-black/80 backdrop-blur-sm print:hidden">
      <PostMetadataBar :date="post?.metadata?.date || post?.date" :stats="readingStats" />
    </div>

    <article v-if="post" class="h-entry">
      <header class="crowned-hero">
        <h1 class="crowned-title" v-html="renderedTitle" />
        <p v-if="post?.metadata?.dek || post?.dek" class="crowned-dek">
          {{ post?.metadata?.dek || post?.dek }}
        </p>
        <WidgetsScrollIndicator :color="palette.accentDim" />
      </header>

      <time v-if="post?.metadata?.date" :datetime="post.metadata.date" class="dt-published hidden">{{ post.metadata.date }}</time>
      <div class="p-author h-card hidden"><span class="p-name">EJ Fox</span><a class="u-url" href="https://ejfox.com">ejfox.com</a></div>
      <a :href="postUrl" class="u-url hidden">{{ postUrl }}</a>

      <div ref="articleContent" class="crowned-body">
        <div v-if="post?.html" class="blog-post-content e-content font-serif" v-html="post.html" />
      </div>

      <div v-if="articleTags.length" class="crowned-tags">
        <a v-for="tag in articleTags" :key="tag" :href="`/blog/tag/${tag}`" class="crowned-tag">{{ tag }}</a>
      </div>

      <PostNav class="print:hidden" :prev-post="nextPrevPosts?.prev" :next-post="nextPrevPosts?.next" />
      <PostRelated class="print:hidden" :related-posts="relatedPosts" />
      <Webmentions class="print:hidden" :url="postUrl" />
    </article>
  </div>
</template>

<style lang="postcss">
body.__BODY_CLASS__ { background: var(--pt-bg) !important; }
body.__BODY_CLASS__ #app-container,
body.__BODY_CLASS__ #app-container > section { background: var(--pt-bg) !important; }
body.__BODY_CLASS__ nav.sticky,
body.__BODY_CLASS__ nav.md\:hidden { display: none !important; }
body.__BODY_CLASS__ #main-content { width: 100% !important; padding-top: 0 !important; }
body.__BODY_CLASS__ footer { background: var(--pt-bg) !important; border-color: transparent !important; }
body.__BODY_CLASS__ .blog-post-content blockquote::before,
body.__BODY_CLASS__ blockquote::before { content: none !important; display: none !important; }

.__CSS_CLASS__ {
  position: relative; min-height: 100vh;
  background: var(--pt-bg); color: var(--pt-text-dim);
}

.crowned-progress { position: fixed; top: 0; left: 0; right: 0; height: 1px; z-index: 101; }
.crowned-progress-inner { height: 100%; background: linear-gradient(90deg, var(--pt-accent-dim), var(--pt-accent-glow)); transition: width 75ms ease-out; }

.crowned-hero {
  position: relative; z-index: 1; max-width: 900px; margin: 0 auto;
  padding: 8rem 2rem 6rem; min-height: 70vh;
  display: flex; flex-direction: column; justify-content: center;
}
@media (min-width: 768px) { .crowned-hero { padding: 10rem 2rem 8rem; min-height: 80vh; } }

.crowned-title {
  font-size: clamp(2.5rem, 8vw, 5.5rem); font-weight: 900; line-height: 1.05;
  letter-spacing: -0.03em; color: var(--pt-text); overflow-wrap: break-word;
}
.crowned-title .typing-char { display: inline; opacity: 0; transition: opacity 0.06s ease-out; }
.crowned-title .typing-char.typed { opacity: 1; }
.crowned-title .cursor {
  display: inline-block; width: 3px; height: 0.85em; margin-left: 1px;
  background: var(--pt-accent); animation: crowned-blink 0.5s ease-in-out infinite; vertical-align: baseline;
}
@keyframes crowned-blink { 0%, 40% { opacity: 0.85; } 50%, 90% { opacity: 0; } 100% { opacity: 0.85; } }

.crowned-dek {
  margin-top: 1.5rem; font-family: Georgia, serif; font-size: 1.125rem;
  line-height: 1.6; color: var(--pt-text-muted); max-width: 42em; font-style: italic;
}

.crowned-body { max-width: 900px; margin: 0 auto; padding: 0 2rem 4rem; }
.__CSS_CLASS__ .blog-post-content { --body-margin: 2rem; }

.__CSS_CLASS__ .blog-post-content p,
.__CSS_CLASS__ .blog-post-content ul,
.__CSS_CLASS__ .blog-post-content ol,
.__CSS_CLASS__ .blog-post-content li { color: var(--pt-text-dim); }

.__CSS_CLASS__ .blog-post-content h1,
.__CSS_CLASS__ .blog-post-content h2 { color: var(--pt-text); border: none !important; }
.__CSS_CLASS__ .blog-post-content h3,
.__CSS_CLASS__ .blog-post-content h4 { color: var(--pt-text-dim); }

.__CSS_CLASS__ .blog-post-content a { color: var(--pt-accent); text-decoration-color: var(--pt-accent-faint); }
.__CSS_CLASS__ .blog-post-content a:hover { color: var(--pt-accent-glow); text-shadow: 0 0 12px var(--pt-accent-faint); }

.__CSS_CLASS__ .blog-post-content blockquote {
  border-left: 2px solid var(--pt-accent-faint) !important;
  background: transparent !important; padding: 0.5rem 0 0.5rem 1.5rem !important;
  color: var(--pt-text-dim);
}

.__CSS_CLASS__ .blog-post-content code { background: var(--pt-surface); color: var(--pt-accent-glow); border: 1px solid var(--pt-accent-faint); }
.__CSS_CLASS__ .blog-post-content pre { background: var(--pt-surface); border: none; }
.__CSS_CLASS__ .blog-post-content pre code { background: transparent; border: none; }
.__CSS_CLASS__ .blog-post-content img { border-radius: 6px; }
.__CSS_CLASS__ .blog-post-content hr { border: none; height: 1px; margin: 4rem 0; background: linear-gradient(90deg, transparent, var(--pt-accent-dim), transparent); }

.crowned-tags { max-width: 900px; margin: 0 auto; padding: 1rem 2rem 2rem; display: flex; flex-wrap: wrap; gap: 0.5rem; }
.crowned-tag {
  font-family: ui-monospace, monospace; font-size: 0.6875rem;
  padding: 0.25rem 0.75rem; border-radius: 999px;
  border: 1px solid var(--pt-accent-faint); color: var(--pt-text-muted); text-decoration: none; transition: all 0.15s;
}
.crowned-tag:hover { border-color: var(--pt-accent-dim); color: var(--pt-text); }

@media (prefers-reduced-motion: reduce) {
  .crowned-title .typing-char { opacity: 1 !important; transition: none; }
  .crowned-title .cursor { display: none; }
}

@media print {
  .crowned-progress { display: none !important; }
  .__CSS_CLASS__ { background: white !important; color: black !important; }
}
</style>
