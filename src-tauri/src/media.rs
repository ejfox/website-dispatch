//! Media upload orchestrator.
//!
//! One drop handler call → primary destination (URL goes in markdown) plus
//! best-effort mirrors. Adding a new destination is "implement upload + a
//! match arm in `upload_to_kind`."

use serde::{Deserialize, Serialize};

use crate::config::MediaDestinationKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadedAsset {
    pub destination: MediaDestinationKind,
    pub url: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorOutcome {
    pub destination: MediaDestinationKind,
    pub success: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaUploadResult {
    pub success: bool,
    pub asset: Option<UploadedAsset>,
    /// Status of each mirror destination, in the order they were configured.
    pub mirrors: Vec<MirrorOutcome>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaStatus {
    pub cloudinary: bool,
    pub r2: bool,
    pub primary: MediaDestinationKind,
    pub mirrors: Vec<MediaDestinationKind>,
}

/// Upload to one destination and return the resulting public URL.
async fn upload_to_kind(
    kind: MediaDestinationKind,
    file_path: &str,
    folder: Option<&str>,
) -> Result<UploadedAsset, String> {
    match kind {
        MediaDestinationKind::Cloudinary => {
            let result = crate::cloudinary::upload_file(file_path, folder, None).await?;
            if result.success {
                let asset = result
                    .asset
                    .ok_or_else(|| "Cloudinary returned success with no asset".to_string())?;
                Ok(UploadedAsset {
                    destination: MediaDestinationKind::Cloudinary,
                    url: asset.secure_url,
                    bytes: asset.bytes,
                })
            } else {
                Err(result.error.unwrap_or_else(|| "Cloudinary upload failed".into()))
            }
        }
        MediaDestinationKind::R2 => {
            let asset = crate::r2::upload_file(file_path, folder).await?;
            Ok(UploadedAsset {
                destination: MediaDestinationKind::R2,
                url: asset.url,
                bytes: asset.bytes,
            })
        }
    }
}

/// Run the full primary + mirrors fan-out for a single file. Primary failure
/// aborts the operation; mirror failures are recorded but non-fatal.
pub async fn upload(file_path: &str, folder: Option<&str>) -> MediaUploadResult {
    let media_config = match crate::config::get() {
        Ok(c) => c.media,
        Err(e) => {
            return MediaUploadResult {
                success: false,
                asset: None,
                mirrors: Vec::new(),
                error: Some(format!("Config unavailable: {}", e)),
            };
        }
    };

    let primary_result = upload_to_kind(media_config.primary, file_path, folder).await;
    let primary_asset = match primary_result {
        Ok(a) => a,
        Err(e) => {
            return MediaUploadResult {
                success: false,
                asset: None,
                mirrors: Vec::new(),
                error: Some(format!("{:?} upload failed: {}", media_config.primary, e)),
            };
        }
    };

    let mut mirror_outcomes = Vec::with_capacity(media_config.mirrors.len());
    for mirror_kind in media_config.mirrors {
        // Don't double-upload to the primary destination if a user accidentally
        // also listed it as a mirror.
        if mirror_kind == media_config.primary {
            continue;
        }
        match upload_to_kind(mirror_kind, file_path, folder).await {
            Ok(a) => mirror_outcomes.push(MirrorOutcome {
                destination: mirror_kind,
                success: true,
                url: Some(a.url),
                error: None,
            }),
            Err(e) => {
                log::warn!("Mirror upload to {:?} failed: {}", mirror_kind, e);
                mirror_outcomes.push(MirrorOutcome {
                    destination: mirror_kind,
                    success: false,
                    url: None,
                    error: Some(e),
                });
            }
        }
    }

    MediaUploadResult {
        success: true,
        asset: Some(primary_asset),
        mirrors: mirror_outcomes,
        error: None,
    }
}

/// Cheap probe of both destinations (in parallel) so the Settings UI can
/// render green/red dots and the user knows what's reachable before they drop.
pub async fn status() -> MediaStatus {
    let media_config = crate::config::get().map(|c| c.media).unwrap_or_default();
    let (cloudinary_ok, r2_ok) = tokio::join!(
        crate::cloudinary::check_credentials(),
        crate::r2::check_credentials(),
    );
    MediaStatus {
        cloudinary: cloudinary_ok,
        r2: r2_ok,
        primary: media_config.primary,
        mirrors: media_config.mirrors,
    }
}
