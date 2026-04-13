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

export interface AppConfig {
  version: number
  vault: VaultConfig
  publish_targets: PublishTarget[]
  editors: EditorConfig[]
  default_editor: string
  cloudinary_cloud_name: string | null
  analytics_url: string | null
  mastodon_instance: string | null
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
