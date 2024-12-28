use std::path::Path;

use rustic_backend::BackendOptions;
use rustic_core::{
    repofile::SnapshotFile, BackupOptions, ConfigOptions, KeyOptions, PathList, Repository, RepositoryOptions, SnapshotOptions
};
use rustix::path::Arg;

pub fn generate_repo(password: String, source: String) -> Repository<rustic_core::NoProgressBars, rustic_core::OpenStatus> {
    let repo_opts = RepositoryOptions::default().password(password);

    println!("Setting backends");

    let backends = BackendOptions::default()
        .repository(source)
        .to_backends()
        .unwrap();

    let key_opts = KeyOptions::default();

    let config_opts = ConfigOptions::default();

    println!("Initialising...");
    Repository::new(&repo_opts, &backends.clone())
        .unwrap()
        .init(&key_opts, &config_opts)
        .unwrap()
}

#[test]
pub fn generate_and_push() {

    println!("Creating file");
    let file = tempfile::tempdir().unwrap();
    println!("Created file");

    let source = file.into_path().as_str().unwrap().to_string();
    let password = String::from("test");

    println!("Source directory at {}", source);
    
    println!("Initialising repo");
    let _repo = generate_repo(password.clone(), source.clone());
    println!("Repo initialised");
    let repo_opts = RepositoryOptions::default().password(password);

    let backends = BackendOptions::default()
        .repository(source)
        .to_backends()
        .unwrap();

    println!("Opening repo");
    let repo = Repository::new(&repo_opts, &backends).unwrap().open().unwrap();
    println!("Repo created");

    let snaps = repo.get_all_snapshots().unwrap();
    // Should be zero, as the repository has just been initialized
    assert_eq!(snaps.len(), 0);

    // Turn repository state to indexed (for backup):
    let repo = repo.to_indexed_ids().unwrap();

    let snap = SnapshotOptions::default()
        .add_tags("tag1,tag2").unwrap()
        .to_snapshot().unwrap();

    let backup_opts = BackupOptions::default();
    let source = PathList::from_iter(["."].iter()).sanitize().unwrap();

    let snap = repo.backup(&backup_opts, &source, snap).unwrap();

    let snaps = repo.get_all_snapshots().unwrap();
    assert_eq!(snaps.len(), 1);

    assert_eq!(snaps[0], snap);
}
