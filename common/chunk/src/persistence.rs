use std::{
    collections::{HashMap, hash_map::Entry},
    fs::File,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle},
};

use anyhow::{Context, Error};
use spatial::vectors::Vec2iChunk;
use zstd::stream::{decode_all, encode_all};

use crate::{Chunk, ChunkMap, ChunkProvider, ChunkState, ChunkStore, WireChunk};

const CHUNK_SHARD_EDGE: usize = 16;
const CHUNK_SHARD_EDGE_I32: i32 = CHUNK_SHARD_EDGE as i32;
const CHUNK_PAGE_EDGE: usize = 4;
const CHUNK_PAGE_EDGE_I32: i32 = CHUNK_PAGE_EDGE as i32;

const SHARD_FILE_MAGIC: [u8; 4] = *b"JSCZ";
const SHARD_FILE_VERSION: u8 = 1;

pub struct ChunkPersistence {
    request_tx: Sender<Request>,
    loaded_rx: Receiver<WorkerLoadResult>,
    worker: Option<JoinHandle<()>>,
}

pub enum DequeuedChunk {
    Loaded(Chunk),
    Missing(Vec2iChunk),
    Error {
        coordinate: Vec2iChunk,
        message: String,
    },
}

enum Request {
    Save { wire: WireChunk },
    Load { coordinate: Vec2iChunk },
    Flush { done_tx: Sender<()> },
    Shutdown,
}

enum WorkerLoadResult {
    Loaded {
        wire: WireChunk,
    },
    Missing {
        coordinate: Vec2iChunk,
    },
    Error {
        coordinate: Vec2iChunk,
        message: String,
    },
}

#[derive(Default)]
struct ShardData {
    chunks: HashMap<Vec2iChunk, Vec<u8>>,
    dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ShardCoord {
    x: i32,
    z: i32,
}

impl ShardCoord {
    fn identifier(&self) -> u64 {
        (self.x as u64).wrapping_mul(2654435761) ^ (self.z as u64).wrapping_mul(2246822519)
    }
}

fn shard_coord_for(coordinate: Vec2iChunk) -> ShardCoord {
    ShardCoord {
        x: coordinate.x().div_euclid(CHUNK_SHARD_EDGE_I32),
        z: coordinate.z().div_euclid(CHUNK_SHARD_EDGE_I32),
    }
}

fn shard_local_chunk(coordinate: Vec2iChunk) -> (usize, usize) {
    (
        coordinate.x().rem_euclid(CHUNK_SHARD_EDGE_I32) as usize,
        coordinate.z().rem_euclid(CHUNK_SHARD_EDGE_I32) as usize,
    )
}

fn page_coord_for(coordinate: Vec2iChunk) -> (usize, usize) {
    let (local_x, local_z) = shard_local_chunk(coordinate);
    (local_x / CHUNK_PAGE_EDGE, local_z / CHUNK_PAGE_EDGE)
}

fn shard_path(root: &Path, shard: ShardCoord) -> PathBuf {
    root.join(format!("{:x}", shard.identifier()))
}

fn load_or_create_shard<'a>(
    root: &Path,
    shard_coord: ShardCoord,
    shards: &'a mut HashMap<ShardCoord, ShardData>,
) -> Result<&'a mut ShardData, Error> {
    if let Entry::Vacant(e) = shards.entry(shard_coord) {
        let shard = load_shard_from_disk(&shard_path(root, shard_coord))?;
        e.insert(shard);
    }

    Ok(shards.get_mut(&shard_coord).unwrap())
}

fn flush_shards(root: &Path, shards: &mut HashMap<ShardCoord, ShardData>) -> Result<(), Error> {
    for (shard_coord, shard) in shards.iter_mut() {
        if !shard.dirty {
            continue;
        }

        let path = shard_path(root, *shard_coord);
        write_shard_file(&path, *shard_coord, shard)?;
        shard.dirty = false;
    }

    Ok(())
}

fn load_shard_from_disk(path: &Path) -> Result<ShardData, Error> {
    if !path.exists() {
        return Ok(ShardData::default());
    }

    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("failed to open shard {}", path.display()))?
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read shard {}", path.display()))?;

    let mut cursor = Cursor::new(bytes);

    let mut magic = [0u8; 4];
    cursor.read_exact(&mut magic)?;

    if magic != SHARD_FILE_MAGIC {
        anyhow::bail!("invalid shard file magic");
    }

    let mut version = [0u8; 1];
    cursor.read_exact(&mut version)?;

    if version[0] != SHARD_FILE_VERSION {
        anyhow::bail!("unsupported shard file version {}", version[0]);
    }

    let mut shard_x = [0u8; 4];
    let mut shard_z = [0u8; 4];
    cursor.read_exact(&mut shard_x)?;
    cursor.read_exact(&mut shard_z)?;

    let mut page_count_bytes = [0u8; 4];
    cursor.read_exact(&mut page_count_bytes)?;
    let page_count = u32::from_le_bytes(page_count_bytes) as usize;

    let shard_x = i32::from_le_bytes(shard_x);
    let shard_z = i32::from_le_bytes(shard_z);

    let mut chunks = HashMap::new();

    for _ in 0..page_count {
        let mut page_coord_bytes = [0u8; 2];
        let mut compressed_len_bytes = [0u8; 4];
        let mut uncompressed_len_bytes = [0u8; 4];

        cursor.read_exact(&mut page_coord_bytes)?;
        cursor.read_exact(&mut compressed_len_bytes)?;
        cursor.read_exact(&mut uncompressed_len_bytes)?;

        let compressed_len = u32::from_le_bytes(compressed_len_bytes) as usize;
        let uncompressed_len = u32::from_le_bytes(uncompressed_len_bytes) as usize;
        let page_x = page_coord_bytes[0] as i32;
        let page_z = page_coord_bytes[1] as i32;

        let mut compressed = vec![0u8; compressed_len];
        cursor.read_exact(&mut compressed)?;

        let decompressed = decode_all(Cursor::new(compressed))?;
        if decompressed.len() != uncompressed_len {
            anyhow::bail!("page length mismatch");
        }

        let mut page_cursor = Cursor::new(decompressed);
        let mut chunk_count_bytes = [0u8; 2];
        page_cursor.read_exact(&mut chunk_count_bytes)?;
        let chunk_count = u16::from_le_bytes(chunk_count_bytes) as usize;

        for _ in 0..chunk_count {
            let mut local_bytes = [0u8; 2];
            let mut chunk_len_bytes = [0u8; 4];

            page_cursor.read_exact(&mut local_bytes)?;
            page_cursor.read_exact(&mut chunk_len_bytes)?;

            let chunk_len = u32::from_le_bytes(chunk_len_bytes) as usize;
            let mut chunk_bytes = vec![0u8; chunk_len];
            page_cursor.read_exact(&mut chunk_bytes)?;

            let local_x = local_bytes[0] as usize;
            let local_z = local_bytes[1] as usize;
            let chunk = Vec2iChunk::new(
                shard_x * CHUNK_SHARD_EDGE_I32 + page_x * CHUNK_PAGE_EDGE_I32 + local_x as i32,
                shard_z * CHUNK_SHARD_EDGE_I32 + page_z * CHUNK_PAGE_EDGE_I32 + local_z as i32,
            );

            chunks.insert(chunk, chunk_bytes);
        }
    }

    Ok(ShardData {
        chunks,
        dirty: false,
    })
}

fn write_shard_file(path: &Path, shard: ShardCoord, data: &ShardData) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent dir {}", parent.display()))?;
    }

    let mut pages = Vec::new();
    let page_axis_count = CHUNK_SHARD_EDGE / CHUNK_PAGE_EDGE;
    let mut grouped: HashMap<_, Vec<_>> = HashMap::new();

    for (&coordinate, bytes) in &data.chunks {
        let (local_x, local_z) = shard_local_chunk(coordinate);
        let page_coord = page_coord_for(coordinate);
        grouped.entry(page_coord).or_default().push((
            local_x % CHUNK_PAGE_EDGE,
            local_z % CHUNK_PAGE_EDGE,
            bytes.clone(),
        ));
    }

    for page_z in 0..page_axis_count {
        for page_x in 0..page_axis_count {
            let page = (page_x, page_z);
            let mut entries = grouped.remove(&page).unwrap_or_default();
            entries.sort_by_key(|(x, z, _)| (*x, *z));
            pages.push((page, entries));
        }
    }

    let mut encoded = Vec::new();
    encoded.extend_from_slice(&SHARD_FILE_MAGIC);
    encoded.push(SHARD_FILE_VERSION);
    encoded.extend_from_slice(&shard.x.to_le_bytes());
    encoded.extend_from_slice(&shard.z.to_le_bytes());
    encoded.extend_from_slice(&(pages.len() as u32).to_le_bytes());

    for ((page_x, page_z), entries) in pages {
        let mut uncompressed = Vec::new();
        uncompressed.extend_from_slice(&(entries.len() as u16).to_le_bytes());

        for (local_x, local_z, chunk_bytes) in entries {
            uncompressed.push(local_x as u8);
            uncompressed.push(local_z as u8);
            uncompressed.extend_from_slice(&(chunk_bytes.len() as u32).to_le_bytes());
            uncompressed.extend_from_slice(&chunk_bytes);
        }

        let compressed = encode_all(Cursor::new(uncompressed.as_slice()), 3)
            .with_context(|| "failed to zstd-compress shard page")?;

        encoded.push(page_x as u8);
        encoded.push(page_z as u8);
        encoded.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        encoded.extend_from_slice(&(uncompressed.len() as u32).to_le_bytes());
        encoded.extend_from_slice(&compressed);
    }

    let tmp_path = path.with_extension("tmp");

    {
        let mut file = File::create(&tmp_path)
            .with_context(|| format!("failed to create temp file {}", tmp_path.display()))?;
        file.write_all(&encoded)
            .with_context(|| format!("failed to write temp file {}", tmp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync temp file {}", tmp_path.display()))?;
    }

    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove old file {}", path.display()))?;
    }

    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "failed to move temp file {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

impl ChunkPersistence {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, Error> {
        let root = root.into();
        std::fs::create_dir_all(&root)
            .with_context(|| format!("failed to create persistence dir {}", root.display()))?;

        let (request_tx, request_rx) = mpsc::channel();
        let (loaded_tx, loaded_rx) = mpsc::channel();

        let worker = thread::Builder::new()
            .name("chunk-persistence".to_string())
            .spawn(move || run_worker(root, request_rx, loaded_tx))
            .context("failed to spawn chunk persistence worker")?;

        Ok(Self {
            request_tx,
            loaded_rx,
            worker: Some(worker),
        })
    }

    pub fn enqueue_save(&self, chunk: &Chunk, store: &ChunkStore) -> Result<(), Error> {
        let wire = WireChunk::from_chunk(chunk, store).with_context(|| {
            let coordinate = chunk.coordinate();
            format!(
                "chunk {coordinate:?} references section data that is not present in ChunkStore",
            )
        })?;

        self.request_tx
            .send(Request::Save { wire })
            .context("chunk persistence worker is not running")
    }

    pub fn enqueue_load(&self, coordinate: Vec2iChunk) -> Result<(), Error> {
        self.request_tx
            .send(Request::Load { coordinate })
            .context("chunk persistence worker is not running")
    }

    pub fn try_dequeue_loaded(&self, store: &mut ChunkStore) -> Option<DequeuedChunk> {
        match self.loaded_rx.try_recv() {
            Ok(result) => Some(materialize_loaded(result, store)),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn dequeue_loaded(&self, store: &mut ChunkStore) -> Result<DequeuedChunk, Error> {
        let result = self
            .loaded_rx
            .recv()
            .context("chunk persistence worker stopped before returning a load result")?;

        Ok(materialize_loaded(result, store))
    }

    pub fn flush(&self) -> Result<(), Error> {
        let (done_tx, done_rx) = mpsc::channel();

        self.request_tx
            .send(Request::Flush { done_tx })
            .context("chunk persistence worker is not running")?;

        done_rx
            .recv()
            .context("chunk persistence worker stopped before flush completed")
    }
}

impl Drop for ChunkPersistence {
    fn drop(&mut self) {
        let _ = self.request_tx.send(Request::Shutdown);

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn materialize_loaded(result: WorkerLoadResult, store: &mut ChunkStore) -> DequeuedChunk {
    match result {
        WorkerLoadResult::Loaded { wire } => DequeuedChunk::Loaded(wire.into_chunk(store)),
        WorkerLoadResult::Missing { coordinate } => DequeuedChunk::Missing(coordinate),
        WorkerLoadResult::Error {
            coordinate,
            message,
        } => DequeuedChunk::Error {
            coordinate,
            message,
        },
    }
}

fn run_worker(root: PathBuf, request_rx: Receiver<Request>, loaded_tx: Sender<WorkerLoadResult>) {
    let mut shards = HashMap::new();

    while let Ok(request) = request_rx.recv() {
        match request {
            Request::Save { wire } => {
                let coordinate = wire.coordinate();
                let shard_coord = shard_coord_for(coordinate);

                match load_or_create_shard(&root, shard_coord, &mut shards) {
                    Ok(shard) => {
                        shard.chunks.insert(coordinate, wire.into_bytes());

                        let path = shard_path(&root, shard_coord);
                        if let Err(err) = write_shard_file(&path, shard_coord, shard) {
                            let _ = loaded_tx.send(WorkerLoadResult::Error {
                                coordinate,
                                message: format!("save failed: {err:#}"),
                            });
                            continue;
                        }

                        shard.dirty = false;
                    }
                    Err(err) => {
                        let _ = loaded_tx.send(WorkerLoadResult::Error {
                            coordinate,
                            message: format!("save failed: {err:#}"),
                        });
                    }
                }
            }
            Request::Load { coordinate } => {
                let shard_coord = shard_coord_for(coordinate);
                let result = match load_or_create_shard(&root, shard_coord, &mut shards) {
                    Ok(shard) => match shard.chunks.get(&coordinate) {
                        Some(bytes) => match WireChunk::from_bytes(bytes) {
                            Ok(wire) => WorkerLoadResult::Loaded { wire },
                            Err(err) => WorkerLoadResult::Error {
                                coordinate,
                                message: format!("load failed: {err:#}"),
                            },
                        },
                        None => WorkerLoadResult::Missing { coordinate },
                    },
                    Err(err) => WorkerLoadResult::Error {
                        coordinate,
                        message: format!("load failed: {err:#}"),
                    },
                };

                if loaded_tx.send(result).is_err() {
                    break;
                }
            }
            Request::Flush { done_tx } => {
                if let Err(err) = flush_shards(&root, &mut shards) {
                    tracing::warn!("chunk persistence flush failed: {err:#}");
                }
                let _ = done_tx.send(());
            }
            Request::Shutdown => {
                let _ = flush_shards(&root, &mut shards);
                break;
            }
        }
    }
}

impl ChunkMap {
    pub fn new_persistent(root: impl Into<PathBuf>) -> Result<Self, Error> {
        Ok(Self {
            persistence: Some(ChunkPersistence::open(root)?),
            ..Self::default()
        })
    }

    pub(crate) fn insert_chunk_internal(&mut self, chunk: Chunk, mark_dirty: bool) {
        let coordinate = chunk.coordinate();

        if let Some(previous) = self.chunks.insert(coordinate, chunk) {
            self.chunk_store.untrack_chunk(&previous);
        }

        let inserted = self.chunks.get(&coordinate).unwrap();
        self.chunk_store.track_chunk(inserted);

        self.set_chunk_state(coordinate, ChunkState::LOAD_PENDING, false);
        self.set_chunk_state(coordinate, ChunkState::LOAD_MISSING, false);
        self.set_chunk_state(coordinate, ChunkState::DIRTY, mark_dirty);
    }

    fn handle_persistence_event(&mut self, event: DequeuedChunk) {
        match event {
            DequeuedChunk::Loaded(chunk) => {
                self.set_chunk_state(chunk.coordinate(), ChunkState::LOAD_MISSING, false);
                self.insert_chunk_internal(chunk, false)
            }
            DequeuedChunk::Missing(coordinate) => {
                self.set_chunk_state(coordinate, ChunkState::LOAD_PENDING, false);
                self.set_chunk_state(coordinate, ChunkState::LOAD_MISSING, true);
            }
            DequeuedChunk::Error {
                coordinate,
                message,
            } => {
                self.set_chunk_state(coordinate, ChunkState::LOAD_PENDING, false);
                self.set_chunk_state(coordinate, ChunkState::LOAD_MISSING, true);
                tracing::warn!("Chunk persistence error at {coordinate:?}: {message}");
            }
        }
    }

    pub fn poll_persistence(&mut self) {
        loop {
            let Some(event) = ({
                let Some(persistence) = &self.persistence else {
                    return;
                };

                persistence.try_dequeue_loaded(&mut self.chunk_store)
            }) else {
                return;
            };

            self.handle_persistence_event(event);
        }
    }

    pub fn get_or_generate_chunk<F: FnOnce(&mut ChunkStore, Vec2iChunk) -> Chunk>(
        &mut self,
        coordinate: Vec2iChunk,
        generate: F,
    ) -> Result<Option<WireChunk>, Error> {
        self.poll_persistence();

        if self.contains_chunk(coordinate) {
            let chunk = self.chunk(coordinate).unwrap();
            let wire = WireChunk::from_chunk(chunk, self.store());
            return Ok(wire);
        }

        if self.take_chunk_state(coordinate, ChunkState::LOAD_MISSING) {
            let chunk = generate(&mut self.chunk_store, coordinate);
            self.insert_chunk_internal(chunk, true);

            let wire = self
                .chunk(coordinate)
                .and_then(|chunk| WireChunk::from_chunk(chunk, self.store()));
            return Ok(wire);
        }

        if self.persistence.is_some() {
            if !self.has_chunk_state(coordinate, ChunkState::LOAD_PENDING) {
                self.set_chunk_state(coordinate, ChunkState::LOAD_PENDING, true);
                let persistence = self.persistence.as_ref().unwrap();
                persistence.enqueue_load(coordinate)?;
            }

            return Ok(None);
        }

        let chunk = generate(&mut self.chunk_store, coordinate);
        self.insert_chunk_internal(chunk, true);

        let wire = self
            .chunk(coordinate)
            .and_then(|chunk| WireChunk::from_chunk(chunk, self.store()));
        Ok(wire)
    }

    pub fn mark_chunk_dirty(&mut self, coordinate: Vec2iChunk) {
        if self.contains_chunk(coordinate) {
            self.set_chunk_state(coordinate, ChunkState::DIRTY, true);
        }
    }

    pub fn persist_dirty(&mut self, max_chunks: usize) -> Result<usize, Error> {
        self.poll_persistence();

        if self.persistence.is_none() {
            let dirty: Vec<_> = self
                .chunk_states
                .iter()
                .filter_map(|(&coordinate, state)| {
                    state.has(ChunkState::DIRTY).then_some(coordinate)
                })
                .collect();

            for coordinate in dirty {
                self.set_chunk_state(coordinate, ChunkState::DIRTY, false);
            }

            return Ok(0);
        }

        let candidates: Vec<_> = self
            .chunk_states
            .iter()
            .filter_map(|(&coordinate, state)| state.has(ChunkState::DIRTY).then_some(coordinate))
            .take(max_chunks)
            .collect();

        let mut persisted = 0;
        for coordinate in candidates {
            let Some(chunk) = self.chunks.get(&coordinate) else {
                self.set_chunk_state(coordinate, ChunkState::DIRTY, false);
                continue;
            };

            {
                let persistence = self.persistence.as_ref().unwrap();
                persistence.enqueue_save(chunk, &self.chunk_store)?;
            }
            self.set_chunk_state(coordinate, ChunkState::DIRTY, false);
            persisted += 1;
        }

        Ok(persisted)
    }
}
