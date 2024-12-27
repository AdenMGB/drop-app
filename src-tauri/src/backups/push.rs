use std::path::Path;

use rustic_backend::BackendOptions;
use rustic_core::{BackupOptions, ConfigOptions, KeyOptions, PathList,
    Repository, RepositoryOptions, SnapshotOptions
};

use super::backup_manager::BackupManager;

pub fn push(manager: &BackupManager) {
    let repo_opts = RepositoryOptions::default()
        .password("test");

    let backends = BackendOptions::default()
        .repository(manager.source.clone())
        .to_backends()
        .unwrap();

    let key_opts = KeyOptions::default();

    let config_opts = ConfigOptions::default();

    let _repo = Repository::new(&repo_opts, &backends.clone()).unwrap().init(&key_opts, &config_opts).unwrap();

    let repo = Repository::new(&repo_opts, &backends).unwrap().open().unwrap();

    let snaps = repo.get_all_snapshots().unwrap();
    assert_eq!(snaps.len(), 0);


    let repo = repo.to_indexed_ids().unwrap();

    let snap = SnapshotOptions::default()
        .add_tags("tag1,tag2").unwrap()
        .to_snapshot().unwrap();

    let backup_opts = BackupOptions::default();
    let source = PathList::from_string("src").unwrap().sanitize().unwrap();

    let snap = repo.backup(&backup_opts, &source, snap).unwrap();

    let snaps = repo.get_all_snapshots().unwrap();
    assert_eq!(snaps.len(), 1);

    assert_eq!(snaps[0], snap);
}