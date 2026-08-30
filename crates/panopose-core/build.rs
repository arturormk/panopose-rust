use std::{env, fs, path::PathBuf, process::Command};

#[path = "src/version_logic.rs"]
mod version_logic;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("panopose-core lives under crates/")
        .to_path_buf();
    let manifest_version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");

    watch_git_head(&workspace_dir);

    let version =
        version_logic::display_version(&manifest_version, git_commit_count(&workspace_dir));
    println!("cargo:rustc-env=PANPOSE_APP_VERSION={version}");
}

fn watch_git_head(workspace_dir: &PathBuf) {
    let git_dir = workspace_dir.join(".git");
    let head_path = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head_path.display());

    let Ok(head) = fs::read_to_string(&head_path) else {
        return;
    };
    let Some(ref_name) = head.trim().strip_prefix("ref: ") else {
        return;
    };
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join(ref_name).display()
    );
}

fn git_commit_count(workspace_dir: &PathBuf) -> Option<u64> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace_dir)
        .arg("rev-list")
        .arg("--count")
        .arg("HEAD")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}
