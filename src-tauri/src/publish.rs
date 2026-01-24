use crate::Config;
use std::fs;
use std::process::Command;

fn check_git_status(repo_path: &str) -> Result<(), String> {
    // Check if we're in a git repo
    let status = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Not a git repository: {}", e))?;

    if !status.status.success() {
        return Err("Not a git repository".into());
    }

    // Check for uncommitted changes - just log, don't block
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Git status failed: {}", e))?;

    let output = String::from_utf8_lossy(&status.stdout);
    let dirty_files: Vec<&str> = output
        .lines()
        .filter(|l| !l.contains("content/blog/"))
        .collect();

    if !dirty_files.is_empty() {
        eprintln!("Note: {} uncommitted changes in repo (continuing anyway)", dirty_files.len());
    }

    // Check if we're on a branch
    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Git branch check failed: {}", e))?;

    let branch_name = String::from_utf8_lossy(&branch.stdout).trim().to_string();
    if branch_name.is_empty() {
        return Err("Detached HEAD state - please checkout a branch".into());
    }

    // Check for merge conflicts
    let conflicts = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Git conflict check failed: {}", e))?;

    if !String::from_utf8_lossy(&conflicts.stdout).trim().is_empty() {
        return Err("Merge conflicts detected - please resolve before publishing".into());
    }

    // Fetch to check if we're behind (non-blocking check)
    let _ = Command::new("git")
        .args(["fetch", "--dry-run"])
        .current_dir(repo_path)
        .output();

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GitStatus {
    pub ok: bool,
    pub branch: String,
    pub error: Option<String>,
    pub dirty_files: Vec<String>,
    pub has_conflicts: bool,
}

pub fn get_git_status() -> GitStatus {
    let config = Config::default();
    let repo_path = &config.website_repo;

    // Get branch name
    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(repo_path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // Check for dirty files
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output();

    let dirty_files: Vec<String> = status
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.contains("content/blog/"))
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();

    // Check for conflicts
    let conflicts = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_path)
        .output()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    let error = if conflicts {
        Some("Merge conflicts - resolve before publishing".into())
    } else if !dirty_files.is_empty() {
        Some(format!("{} uncommitted changes", dirty_files.len()))
    } else if branch.is_empty() {
        Some("Detached HEAD".into())
    } else {
        None
    };

    GitStatus {
        ok: error.is_none(),
        branch,
        error,
        dirty_files,
        has_conflicts: conflicts,
    }
}

pub fn publish_file(source_path: &str, slug: &str) -> Result<String, String> {
    let config = Config::default();

    // Pre-flight checks
    eprintln!("Running pre-flight checks...");
    check_git_status(&config.website_repo)?;

    // Determine year folder (posts go in content/blog/blog/{year}/)
    let year = chrono::Utc::now().format("%Y").to_string();
    let dest_dir = format!("{}/content/blog/blog/{}", config.website_repo, year);
    let dest_path = format!("{}/{}.md", dest_dir, slug);

    eprintln!("Publishing {} -> {}", source_path, dest_path);

    // Ensure year directory exists
    fs::create_dir_all(&dest_dir).map_err(|e| format!("Failed to create dir: {}", e))?;

    // Copy file
    fs::copy(source_path, &dest_path).map_err(|e| format!("Failed to copy: {}", e))?;

    eprintln!("Copied file, running git commands...");

    // Git add, commit, push
    let repo_path = &config.website_repo;

    let add_output = Command::new("git")
        .args(["add", &dest_path])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Git add failed: {}", e))?;

    if !add_output.status.success() {
        return Err(format!("Git add failed: {}", String::from_utf8_lossy(&add_output.stderr)));
    }

    let commit_msg = format!("Publish: {}", slug);
    let commit_output = Command::new("git")
        .args(["commit", "-m", &commit_msg])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Git commit failed: {}", e))?;

    // Commit might "fail" if nothing changed - that's okay for republish
    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        let stdout = String::from_utf8_lossy(&commit_output.stdout);
        // Check if it's just "nothing to commit"
        if !stdout.contains("nothing to commit") && !stderr.contains("nothing to commit") {
            eprintln!("Git commit output: {}", stdout);
            eprintln!("Git commit stderr: {}", stderr);
            // Continue anyway - file was still copied
        }
    }

    eprintln!("Pulling latest changes...");

    let pull_output = Command::new("git")
        .args(["pull", "--rebase", "--autostash"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Git pull failed: {}", e))?;

    if !pull_output.status.success() {
        let stderr = String::from_utf8_lossy(&pull_output.stderr);
        let stdout = String::from_utf8_lossy(&pull_output.stdout);
        eprintln!("Git pull stdout: {}", stdout);
        eprintln!("Git pull stderr: {}", stderr);

        // If pull failed, try to abort any in-progress rebase
        let _ = Command::new("git")
            .args(["rebase", "--abort"])
            .current_dir(repo_path)
            .output();

        return Err(format!("Git pull failed: {}\n{}", stdout, stderr));
    }

    eprintln!("Pushing to remote...");

    let push_output = Command::new("git")
        .args(["push"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Git push failed: {}", e))?;

    if !push_output.status.success() {
        let stderr = String::from_utf8_lossy(&push_output.stderr);
        // Check if it's "everything up-to-date" which is fine
        if !stderr.contains("Everything up-to-date") && !stderr.contains("up to date") {
            return Err(format!("Git push failed: {}", stderr));
        }
    }

    eprintln!("Published successfully!");

    // Return the URL
    let url = format!("https://ejfox.com/blog/{}/{}", year, slug);
    Ok(url)
}
