use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{DateTime, Utc};
use clap::Parser;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TagInfo {
    name: String,
    commit_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Parser)]
struct Args {
    #[clap(env)]
    remote_url: String,

    #[clap(env, default_value = "tag_state.json")]
    state_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    pub tags: Vec<TagInfo>,
}

fn main() -> eyre::Result<()> {
    let args = Args::parse();

    let tempdir = TempDir::new()?;
    let repo_path = tempdir.path().join("foundry");

    clone_repo(tempdir.path(), &args.remote_url)?;

    let mut state = State::load(&args.state_file)?;

    // Get remote tags
    let remote_tags = fetch_tags(&repo_path)?;

    println!("{:?}", remote_tags);

    // Process new tags
    let tag_set = state
        .tags
        .iter()
        .map(|tag| tag.name.clone())
        .collect::<HashSet<_>>();

    for tag in remote_tags {
        if !tag_set.contains(&tag.name) {
            create_and_push_commit(&mut state, &args, &tag)?;
        }
    }

    Ok(())
}

fn create_and_push_commit(state: &mut State, args: &Args, tag: &TagInfo) -> eyre::Result<()> {
    state.tags.push(tag.clone());
    state.save(&args.state_file)?;

    // git add
    let output = Command::new("git")
        .arg("add")
        .arg("-u")
        .arg(".")
        .spawn()?
        .wait_with_output()?;
    if !output.status.success() {
        eyre::bail!("Failed to add files");
    }

    // git commit
    let output = Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg(format!("Add tag {}", tag.name))
        .spawn()?
        .wait_with_output()?;
    if !output.status.success() {
        eyre::bail!("Failed to commit");
    }

    // git push
    let output = Command::new("git")
        .arg("push")
        .spawn()?
        .wait_with_output()?;
    if !output.status.success() {
        eyre::bail!("Failed to push");
    }

    Ok(())
}

impl State {
    pub fn save(&self, path: impl AsRef<Path>) -> eyre::Result<()> {
        let path = path.as_ref();

        let content = serde_json::to_string(&self)?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, content)?;

        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> eyre::Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            Ok(Self { tags: vec![] })
        } else {
            let content = fs::read_to_string(path)?;

            let state = serde_json::from_str(&content)?;

            Ok(state)
        }
    }
}

fn clone_repo(working_dir: impl AsRef<Path>, remote_url: &str) -> eyre::Result<()> {
    let mut cmd = Command::new("git")
        .current_dir(working_dir.as_ref())
        .arg("clone")
        .arg(remote_url)
        .spawn()?;

    let status = cmd.wait()?;

    if !status.success() {
        eyre::bail!("Failed to clone repo");
    }

    Ok(())
}

fn fetch_tags(repo_path: impl AsRef<Path>) -> eyre::Result<Vec<TagInfo>> {
    let cmd = Command::new("git")
        .current_dir(repo_path.as_ref())
        .arg("for-each-ref")
        .arg("--format='%(refname:strip=2),%(committerdate:iso-strict)'")
        .arg("refs/tags")
        .stdout(Stdio::piped())
        .spawn()?;

    let out = cmd.wait_with_output()?;

    if !out.status.success() {
        eyre::bail!("Failed to fetch tags");
    }

    let output = String::from_utf8(out.stdout)?;

    println!("{output}");

    let tags = output
        .lines()
        .map(|line| {
            let line = line.trim().trim_matches('\'');

            let mut parts = line.split(',');
            let name = parts.next().unwrap().to_string();
            let date = parts.next().unwrap().to_string();

            println!("`{}`", name);
            println!("`{}`", date);

            let date = DateTime::parse_from_rfc3339(&date)?;

            Ok(TagInfo {
                name,
                commit_time: date.into(),
            })
        })
        .collect::<eyre::Result<Vec<_>>>()?;

    Ok(tags)
}
