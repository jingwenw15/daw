//! Project version-control primitives.

use std::{
    fmt, fs, io,
    path::Path,
    process::{Command, Output},
};

/// Crate version exposed for smoke tests and diagnostics.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Git status summary for a project repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitStatus {
    /// True when the project directory has a Git repository.
    pub repository_exists: bool,
    /// Short status lines reported by Git.
    pub lines: Vec<String>,
}

/// Remote repository configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitRemote {
    /// Remote name, commonly `origin`.
    pub name: String,
    /// Remote URL. SSH and HTTPS private remotes are both supported by system Git.
    pub url: String,
}

/// Error returned by version-control operations.
#[derive(Debug)]
pub enum VcsError {
    /// Filesystem failure.
    Io(io::Error),
    /// Git command failed.
    Git {
        /// Command that failed.
        command: String,
        /// Exit code when available.
        code: Option<i32>,
        /// Standard error output.
        stderr: String,
    },
}

/// Initialize a Git repository for a DAW project.
///
/// # Errors
///
/// Returns an error if Git is unavailable, repository initialization fails, or
/// ignore/LFS metadata cannot be written.
pub fn init_git(project_dir: &Path) -> Result<(), VcsError> {
    run_git(project_dir, ["init"])?;
    ensure_gitignore(project_dir)?;
    if git_lfs_available(project_dir) {
        write_lfs_attributes(project_dir)?;
    }
    Ok(())
}

/// Add or update a Git remote.
///
/// # Errors
///
/// Returns an error if Git cannot set the remote URL.
pub fn add_remote(project_dir: &Path, name: &str, url: &str) -> Result<GitRemote, VcsError> {
    let remotes = run_git(project_dir, ["remote"])?;
    let remote_exists = stdout_lines(&remotes).iter().any(|remote| remote == name);
    if remote_exists {
        run_git(project_dir, ["remote", "set-url", name, url])?;
    } else {
        run_git(project_dir, ["remote", "add", name, url])?;
    }
    Ok(GitRemote {
        name: name.to_owned(),
        url: url.to_owned(),
    })
}

/// Return the current Git status.
///
/// # Errors
///
/// Returns an error if Git status cannot be read.
pub fn status(project_dir: &Path) -> Result<GitStatus, VcsError> {
    let repository_exists = project_dir.join(".git").exists();
    if !repository_exists {
        return Ok(GitStatus {
            repository_exists,
            lines: Vec::new(),
        });
    }

    Ok(GitStatus {
        repository_exists,
        lines: stdout_lines(&run_git(project_dir, ["status", "--short"])?),
    })
}

/// Commit all project changes.
///
/// # Errors
///
/// Returns an error if staging or committing fails.
pub fn commit(project_dir: &Path, message: &str) -> Result<(), VcsError> {
    run_git(project_dir, ["add", "."])?;
    run_git(
        project_dir,
        [
            "-c",
            "user.name=DAW",
            "-c",
            "user.email=daw@example.invalid",
            "commit",
            "-m",
            message,
        ],
    )?;
    Ok(())
}

/// Push the current branch to a remote.
///
/// # Errors
///
/// Returns an error if Git push fails. Private SSH/HTTPS auth is handled by
/// system Git and the user's configured credentials.
pub fn push(project_dir: &Path, remote: &str, branch: &str) -> Result<(), VcsError> {
    run_git(project_dir, ["push", "-u", remote, branch])?;
    Ok(())
}

/// Pull the current branch from a remote.
///
/// # Errors
///
/// Returns an error if Git pull fails. Private SSH/HTTPS auth is handled by
/// system Git and the user's configured credentials.
pub fn pull(project_dir: &Path, remote: &str, branch: &str) -> Result<(), VcsError> {
    run_git(project_dir, ["pull", "--ff-only", remote, branch])?;
    Ok(())
}

/// Return the currently checked-out Git branch.
///
/// # Errors
///
/// Returns an error if Git cannot determine the current branch.
pub fn current_branch(project_dir: &Path) -> Result<String, VcsError> {
    let output = run_git(project_dir, ["branch", "--show-current"])?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Return true when `git lfs` is available through system Git.
#[must_use]
pub fn git_lfs_available(project_dir: &Path) -> bool {
    run_git(project_dir, ["lfs", "version"]).is_ok()
}

/// Write LFS tracking metadata for large media files.
///
/// # Errors
///
/// Returns an error if the attributes file cannot be written.
pub fn write_lfs_attributes(project_dir: &Path) -> Result<(), VcsError> {
    let content = "*.wav filter=lfs diff=lfs merge=lfs -text\n*.aif filter=lfs diff=lfs merge=lfs -text\n*.aiff filter=lfs diff=lfs merge=lfs -text\n*.flac filter=lfs diff=lfs merge=lfs -text\n*.mp3 filter=lfs diff=lfs merge=lfs -text\n";
    fs::write(project_dir.join(".gitattributes"), content)?;
    Ok(())
}

impl fmt::Display for VcsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Git {
                command,
                code,
                stderr,
            } => {
                write!(formatter, "git command failed: {command}")?;
                if let Some(code) = code {
                    write!(formatter, " (exit code {code})")?;
                }
                if !stderr.trim().is_empty() {
                    write!(formatter, "\n{}", stderr.trim())?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for VcsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Git { .. } => None,
        }
    }
}

impl From<io::Error> for VcsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn ensure_gitignore(project_dir: &Path) -> Result<(), VcsError> {
    let path = project_dir.join(".gitignore");
    let content = "cache/\nexports/\n.DS_Store\n";
    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        let mut updated = existing;
        for line in content.lines() {
            if !updated.lines().any(|existing_line| existing_line == line) {
                updated.push_str(line);
                updated.push('\n');
            }
        }
        fs::write(path, updated)?;
    } else {
        fs::write(path, content)?;
    }
    Ok(())
}

fn run_git<const N: usize>(project_dir: &Path, args: [&str; N]) -> Result<Output, VcsError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_dir)
        .output()?;

    if output.status.success() {
        Ok(output)
    } else {
        Err(VcsError::Git {
            command: format!("git {}", args.join(" ")),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        add_remote, commit, current_branch, init_git, status, write_lfs_attributes, GitStatus,
        VERSION,
    };
    use std::{fs, path::PathBuf, process::Command};

    #[test]
    fn exposes_package_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn reports_status_without_repository() {
        let project_dir = temp_project_dir("no-repo");
        fs::create_dir_all(&project_dir).expect("create temp dir");

        let status = status(&project_dir).expect("read status");

        assert_eq!(
            status,
            GitStatus {
                repository_exists: false,
                lines: Vec::new()
            }
        );
        fs::remove_dir_all(project_dir).expect("cleanup");
    }

    #[test]
    fn initializes_and_commits_local_repository() {
        let project_dir = temp_project_dir("repo");
        fs::create_dir_all(&project_dir).expect("create temp dir");
        fs::write(project_dir.join("project.daw.json"), "{}\n").expect("write project");

        init_git(&project_dir).expect("init git");
        let branch = current_branch(&project_dir).expect("current branch");
        let dirty = status(&project_dir).expect("dirty status");
        commit(&project_dir, "initial project").expect("commit project");
        let clean = status(&project_dir).expect("clean status");

        assert!(!branch.is_empty());
        assert!(dirty.repository_exists);
        assert!(!dirty.lines.is_empty());
        assert!(clean.lines.is_empty());
        fs::remove_dir_all(project_dir).expect("cleanup");
    }

    #[test]
    fn adds_local_remote() {
        let project_dir = temp_project_dir("remote");
        let remote_dir = temp_project_dir("remote-bare");
        fs::create_dir_all(&project_dir).expect("create project dir");
        fs::create_dir_all(&remote_dir).expect("create remote dir");
        run_git_raw(&remote_dir, ["init", "--bare"]);

        init_git(&project_dir).expect("init git");
        let remote = add_remote(
            &project_dir,
            "origin",
            remote_dir.to_str().expect("remote path"),
        )
        .expect("add remote");

        assert_eq!(remote.name, "origin");
        assert_eq!(remote.url, remote_dir.to_str().expect("remote path"));
        fs::remove_dir_all(project_dir).expect("cleanup project");
        fs::remove_dir_all(remote_dir).expect("cleanup remote");
    }

    #[test]
    fn writes_lfs_attributes() {
        let project_dir = temp_project_dir("lfs");
        fs::create_dir_all(&project_dir).expect("create temp dir");

        write_lfs_attributes(&project_dir).expect("write attributes");
        let attributes =
            fs::read_to_string(project_dir.join(".gitattributes")).expect("read attributes");

        assert!(attributes.contains("*.wav filter=lfs"));
        assert!(attributes.contains("*.aiff filter=lfs"));
        fs::remove_dir_all(project_dir).expect("cleanup");
    }

    fn temp_project_dir(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("daw-vcs-{test_name}-{}", std::process::id()))
    }

    fn run_git_raw<const N: usize>(project_dir: &std::path::Path, args: [&str; N]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(project_dir)
            .status()
            .expect("run git");
        assert!(status.success());
    }
}
