/** The slug `create_new_post` (lib.rs) makes from a title. Keep them in step:
 *  the New Post sheet shows this as the path you're about to get. */
export function slugifyTitle(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^\p{L}\p{N} ]/gu, ' ')
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .join('-')
}
