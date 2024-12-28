use rustic_backend::BackendOptions;
use rustic_core::{repofile::SnapshotFile, BackupOptions, ConfigOptions, KeyOptions, NoProgressBars, OpenStatus, PathList, Repository, RepositoryOptions};


pub struct BackupManager {
    pub repo: Repository<rustic_core::NoProgressBars, OpenStatus>,
    backup_opts: BackupOptions,

}

impl BackupManager {
    pub fn new(password: String, source: String) -> Self {
        let repo_opts = RepositoryOptions::default()
            .password(password);

        let backends = BackendOptions::default()
            .repository(source)
            .to_backends()
            .unwrap();

        Self {
            repo: Repository::new(&repo_opts, &backends).unwrap().open().unwrap(),
            backup_opts: BackupOptions::default(),
        }
    }
    pub fn backup_file(self, sources: PathList, snapshot: SnapshotFile) {
        self.repo.to_indexed_ids().unwrap().backup(&self.backup_opts, &sources, snapshot).unwrap();
    }
}