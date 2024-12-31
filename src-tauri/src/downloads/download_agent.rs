use crate::auth::generate_authorization_header;
use crate::db::DatabaseImpls;
use crate::download_manager::application_download_error::ApplicationDownloadError;
use crate::download_manager::download_logic::download_game_chunk;
use crate::download_manager::download_manager::DownloadManagerSignal;
use crate::download_manager::download_thread_control_flag::{DownloadThreadControl, DownloadThreadControlFlag};
use crate::download_manager::downloadable::Downloadable;
use crate::download_manager::manifest::{DropDownloadContext, DropManifest};
use crate::download_manager::progress_object::{ProgressHandle, ProgressObject};
use crate::download_manager::stored_manifest::StoredManifest;
use crate::remote::RemoteAccessError;
use crate::DB;
use log::{debug, error, info};
use rayon::ThreadPoolBuilder;
use std::collections::VecDeque;
use std::fs::{create_dir_all, File};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use urlencoding::encode;

#[cfg(target_os = "linux")]
use rustix::fs::{fallocate, FallocateFlags};


pub struct GameDownloadAgent {
    id: String,
    version: String,
    control_flag: DownloadThreadControl,
    contexts: Vec<DropDownloadContext>,
    completed_contexts: VecDeque<usize>,
    manifest: Mutex<Option<DropManifest>>,
    progress: Arc<ProgressObject>,
    sender: Sender<DownloadManagerSignal>,
    stored_manifest: StoredManifest,
}



impl GameDownloadAgent {
    pub fn new(
        id: String,
        version: String,
        target_download_dir: usize,
        sender: Sender<DownloadManagerSignal>,
    ) -> Self {
        // Don't run by default
        let control_flag = DownloadThreadControl::new(DownloadThreadControlFlag::Stop);

        let db_lock = DB.borrow_data().unwrap();
        let base_dir = db_lock.applications.install_dirs[target_download_dir].clone();
        drop(db_lock);

        let base_dir_path = Path::new(&base_dir);
        let data_base_dir_path = base_dir_path.join(id.clone());

        let stored_manifest =
            StoredManifest::generate(id.clone(), version.clone(), data_base_dir_path.clone());

        Self {
            id,
            version,
            control_flag,
            manifest: Mutex::new(None),
            contexts: Vec::new(),
            completed_contexts: VecDeque::new(),
            progress: Arc::new(ProgressObject::new(0, 0, sender.clone())),
            sender,
            stored_manifest,
        }
    }

    // Blocking
    pub fn setup_download(&mut self) -> Result<(), ApplicationDownloadError> {
        self.ensure_manifest_exists()?;
        info!("Ensured manifest exists");

        self.ensure_contexts()?;
        info!("Ensured contexts exists");

        self.control_flag.set(DownloadThreadControlFlag::Go);

        Ok(())
    }

    // Blocking
    pub fn download(&mut self) -> Result<(), ApplicationDownloadError> {
        self.setup_download()?;
        self.set_progress_object_params();
        let timer = Instant::now();
        self.run().map_err(|_| ApplicationDownloadError::DownloadError)?;

        info!(
            "{} took {}ms to download",
            self.id,
            timer.elapsed().as_millis()
        );
        Ok(())
    }

    pub fn ensure_manifest_exists(&self) -> Result<(), ApplicationDownloadError> {
        if self.manifest.lock().unwrap().is_some() {
            return Ok(());
        }

        self.download_manifest()
    }

    fn download_manifest(&self) -> Result<(), ApplicationDownloadError> {
        let base_url = DB.fetch_base_url();
        let manifest_url = base_url
            .join(
                format!(
                    "/api/v1/client/metadata/manifest?id={}&version={}",
                    self.id,
                    encode(&self.version)
                )
                .as_str(),
            )
            .unwrap();

        let header = generate_authorization_header();
        let client = reqwest::blocking::Client::new();
        let response = client
            .get(manifest_url.to_string())
            .header("Authorization", header)
            .send()
            .unwrap();

        if response.status() != 200 {
            return Err(ApplicationDownloadError::Communication(
                RemoteAccessError::ManifestDownloadFailed(
                    response.status(),
                    response.text().unwrap(),
                ),
            ));
        }

        let manifest_download = response.json::<DropManifest>().unwrap();

        if let Ok(mut manifest) = self.manifest.lock() {
            *manifest = Some(manifest_download);
            return Ok(());
        }

        Err(ApplicationDownloadError::Lock)
    }

    fn set_progress_object_params(&self) {
        // Avoid re-setting it
        if self.progress.get_max() != 0 {
            return;
        }

        let length = self.contexts.len();

        let chunk_count = self.contexts.iter().map(|chunk| chunk.length).sum();

        debug!("Setting ProgressObject max to {}", chunk_count);
        self.progress.set_max(chunk_count);
        debug!("Setting ProgressObject size to {}", length);
        self.progress.set_size(length);
        debug!("Setting ProgressObject time to now");
        self.progress.set_time_now();
    }

    pub fn ensure_contexts(&mut self) -> Result<(), ApplicationDownloadError> {
        if !self.contexts.is_empty() {
            return Ok(());
        }

        self.generate_contexts()?;
        Ok(())
    }

    pub fn generate_contexts(&mut self) -> Result<(), ApplicationDownloadError> {
        let manifest = self.manifest.lock().unwrap().clone().unwrap();
        let game_id = self.id.clone();

        let mut contexts = Vec::new();
        let base_path = Path::new(&self.stored_manifest.base_path);
        create_dir_all(base_path).unwrap();

        self.completed_contexts.clear();
        self.completed_contexts
            .extend(self.stored_manifest.get_completed_contexts());

        for (raw_path, chunk) in manifest {
            let path = base_path.join(Path::new(&raw_path));

            let container = path.parent().unwrap();
            create_dir_all(container).unwrap();

            let file = File::create(path.clone()).unwrap();
            let mut running_offset = 0;

            for (index, length) in chunk.lengths.iter().enumerate() {
                contexts.push(DropDownloadContext {
                    file_name: raw_path.to_string(),
                    version: chunk.version_name.to_string(),
                    offset: running_offset,
                    index,
                    game_id: game_id.to_string(),
                    path: path.clone(),
                    checksum: chunk.checksums[index].clone(),
                    length: *length,
                    permissions: chunk.permissions,
                });
                running_offset += *length as u64;
            }

            #[cfg(target_os = "linux")]
            if running_offset > 0 {
                let _ = fallocate(file, FallocateFlags::empty(), 0, running_offset);
            }
        }
        self.contexts = contexts;

        Ok(())
    }

    pub fn run(&mut self) -> Result<(), ()> {
        info!("downloading game: {}", self.id);
        const DOWNLOAD_MAX_THREADS: usize = 1;

        let pool = ThreadPoolBuilder::new()
            .num_threads(DOWNLOAD_MAX_THREADS)
            .build()
            .unwrap();

        let completed_indexes = Arc::new(boxcar::Vec::new());
        let completed_indexes_loop_arc = completed_indexes.clone();

        pool.scope(|scope| {
            for (index, context) in self.contexts.iter().enumerate() {
                let completed_indexes = completed_indexes_loop_arc.clone();

                let progress = self.progress.get(index); // Clone arcs
                let progress_handle = ProgressHandle::new(progress, self.progress.clone());
                // If we've done this one already, skip it
                if self.completed_contexts.contains(&index) {
                    progress_handle.add(context.length);
                    continue;
                }

                let context = context.clone();
                let control_flag = self.control_flag.clone(); // Clone arcs

                let sender = self.sender.clone();

                scope.spawn(move |_| {
                    match download_game_chunk(context.clone(), control_flag, progress_handle) {
                        Ok(res) => {
                            if res {
                                completed_indexes.push(index);
                            }
                        }
                        Err(e) => {
                            error!("{}", e);
                            sender.send(DownloadManagerSignal::Error(e)).unwrap();
                        }
                    }
                });
            }
        });

        let newly_completed = completed_indexes.to_owned();

        let completed_lock_len = {
            for (item, _) in newly_completed.iter() {
                self.completed_contexts.push_front(item);
            }

            self.completed_contexts.len()
        };

        // If we're not out of contexts, we're not done, so we don't fire completed
        if completed_lock_len != self.contexts.len() {
            info!("da for {} exited without completing", self.id.clone());
            self.stored_manifest
                .set_completed_contexts(&self.completed_contexts.clone().into());
            info!("Setting completed contexts");
            self.stored_manifest.write();
            info!("Wrote completed contexts");
            return Ok(());
        }

        // We've completed
        self.sender
            .send(DownloadManagerSignal::Completed(self.id.clone()))
            .unwrap();

        Ok(())
    }
}

impl Downloadable for GameDownloadAgent {
    fn get_progress_object(&self) -> Arc<ProgressObject> {
        self.progress.clone()
    }

    fn id(&self) -> String {
        self.id.clone()
    }
    
    fn download(&mut self) -> Result<(), ApplicationDownloadError> {
        self.download()
    }
    
    fn version(&self) -> String {
        self.version.clone()
    }
    
    fn progress(&self) -> Arc<ProgressObject> {
        self.progress.clone()
    }
    
    fn control_flag(&self) -> DownloadThreadControl {
        self.control_flag.clone()
    }
    
    fn install_dir(&self) -> String {
        self.stored_manifest.base_path.to_str().unwrap().to_owned()
    }
}