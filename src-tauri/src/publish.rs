use crate::Config;
use std::fs;
use std::process::Command;

pub fn publish_file(source_path: &str, slug: &str) -> Result<String, String> {
    let config = Config::default();

    // Determine year folder
    let year = chrono::Utc::now().format("%Y").to_string();
    let dest_dir = format!("{}/content/blog/{}", config.website_repo, year);
    let dest_path = format!("{}/{}.md", dest_dir, slug);

    // Ensure year directory exists
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    // Copy file
    fs::copy(source_path, &dest_path).map_err(|e| e.to_string())?;

    // Git add, commit, push
    let repo_path = &config.website_repo;

    Command::new("git")
        .args(["add", &dest_path])
        .current_dir(repo_path)
        .output()
        .map_err(|e| e.to_string())?;

    let commit_msg = format!("Publish: {}", slug);
    Command::new("git")
        .args(["commit", "-m", &commit_msg])
        .current_dir(repo_path)
        .output()
        .map_err(|e| e.to_string())?;

    Command::new("git")
        .args(["push"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| e.to_string())?;

    // Return the URL
    let url = format!("https://ejfox.com/blog/{}/{}", year, slug);
    Ok(url)
}
