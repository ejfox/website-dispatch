export interface MarkdownFile {
  path: string
  filename: string
  title: string | null
  dek: string | null
  date: string | null
  tags: string[]
  created: number
  modified: number
  word_count: number
  published_word_count: number | null
  is_safe: boolean
  warnings: string[]
  published_url: string | null
  published_date: number | null
  source_dir: string
  unlisted: boolean
  password: string | null
  publish_at: string | null
  content_type: string
}

export interface Backlink {
  path: string
  title: string | null
  context: string
}

export interface LocalMediaRef {
  original_text: string
  path: string
  resolved_path: string | null
  alt_text: string | null
  media_type: string
  line_number: number
}

export interface EditorConfig {
  name: string
  app_name: string
  enabled: boolean
}

export interface PublishTargetConfig {
  id: string
  name: string
  is_default: boolean
}

export interface VaultConfig {
  path: string
  name: string
  excluded_dirs: string[]
  publishable_dirs: string[]
}

export interface PublishTarget {
  name: string
  id: string
  repo_path: string
  domain: string
  content_path_pattern: string
  branch: string
  is_default: boolean
}

export interface DiffLine {
  tag: 'equal' | 'added' | 'removed'
  content: string
  source_line: number | null
  published_line: number | null
}

export interface DiffHunk {
  source_start: number
  published_start: number
  lines: DiffLine[]
}

export interface PublishDiff {
  has_diff: boolean
  source_path: string
  published_path: string | null
  words_added: number
  words_removed: number
  lines_added: number
  lines_removed: number
  hunks: DiffHunk[]
  error: string | null
}

export type MediaDestinationKind = 'cloudinary' | 'r2'

export interface CloudinaryCreds {
  cloud_name: string
  api_key: string
  api_secret: string
}

export interface R2Creds {
  account_id: string
  access_key_id: string
  secret_access_key: string
  bucket: string
  public_url_base: string
}

export interface MediaConfig {
  primary: MediaDestinationKind
  mirrors: MediaDestinationKind[]
  cloudinary: CloudinaryCreds | null
  r2: R2Creds | null
}

export interface MediaStatus {
  cloudinary: boolean
  r2: boolean
  primary: MediaDestinationKind
  mirrors: MediaDestinationKind[]
}

export interface AppConfig {
  version: number
  vault: VaultConfig
  publish_targets: PublishTarget[]
  editors: EditorConfig[]
  default_editor: string
  cloudinary_cloud_name: string | null
  analytics_url: string | null
  mastodon_instance: string | null
  media: MediaConfig
  webmentions_bridgy_fed: boolean
}

export interface GitStatus {
  ok: boolean
  branch: string
  error: string | null
  dirty_files: string[]
  has_conflicts: boolean
}

export interface WebmentionResult {
  target: string
  endpoint: string | null
  status: string
  message: string | null
}

export interface WebmentionReport {
  source: string
  results: WebmentionResult[]
  total_links: number
  sent: number
  no_endpoint: number
  errors: number
}

export interface PostAnalytics {
  pageviews: number
  visitors: number
  visits: number
  bounces: number
  totaltime: number
}
