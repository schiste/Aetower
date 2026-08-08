use std::os::unix::fs::MetadataExt;

use super::*;

pub(super) const STORAGE_PATH_FINGERPRINT_VERSION: u8 = 2;
const STORAGE_PATH_FINGERPRINT_LEN: usize = 126;
const DIRECTORY_SHALLOW_FINGERPRINT_CHILD_LIMIT: u64 = 2_048;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct StorageDirectoryFingerprint {
    pub(super) child_count: u64,
    pub(super) shallow_child_hash: u64,
    pub(super) aggregate_bytes: u64,
    pub(super) complete: bool,
    pub(super) last_event_id: u64,
}

impl StorageDirectoryFingerprint {
    pub(super) fn for_path(
        path: &Path,
        aggregate_bytes: u64,
        complete: bool,
        last_event_id: u64,
    ) -> Self {
        let (child_count, shallow_child_hash, shallow_complete) =
            shallow_directory_fingerprint(path);
        Self {
            child_count,
            shallow_child_hash,
            aggregate_bytes,
            complete: complete && shallow_complete,
            last_event_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StoragePathFingerprint {
    pub(super) kind_code: u8,
    pub(super) mode: u32,
    pub(super) device: u64,
    pub(super) inode: u64,
    pub(super) logical_bytes: u64,
    pub(super) physical_bytes: u64,
    pub(super) modified_millis: u64,
    pub(super) changed_millis: u64,
    pub(super) birth_millis: u64,
    pub(super) flags: u64,
    pub(super) link_count: u64,
    pub(super) directory_child_count: u64,
    pub(super) directory_shallow_hash: u64,
    pub(super) directory_aggregate_bytes: u64,
    pub(super) directory_complete: bool,
    pub(super) last_event_id: u64,
    hash: u64,
}

impl StoragePathFingerprint {
    pub(super) fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self::from_metadata_with_directory(metadata, None)
    }

    pub(super) fn from_metadata_with_directory(
        metadata: &fs::Metadata,
        directory: Option<StorageDirectoryFingerprint>,
    ) -> Self {
        let kind_code = if metadata.file_type().is_symlink() {
            3
        } else if metadata.is_dir() {
            2
        } else if metadata.is_file() {
            1
        } else {
            0
        };
        let modified_millis = metadata_time_millis(metadata.mtime(), metadata.mtime_nsec());
        let changed_millis = metadata_time_millis(metadata.ctime(), metadata.ctime_nsec());
        let directory = directory.unwrap_or_default();
        let mut fingerprint = Self {
            kind_code,
            mode: metadata.mode(),
            device: metadata.dev(),
            inode: metadata.ino(),
            logical_bytes: metadata.len(),
            physical_bytes: metadata.blocks().saturating_mul(512),
            modified_millis,
            changed_millis,
            birth_millis: metadata_birth_millis(metadata).unwrap_or_default(),
            flags: metadata_flags(metadata),
            link_count: metadata.nlink(),
            directory_child_count: directory.child_count,
            directory_shallow_hash: directory.shallow_child_hash,
            directory_aggregate_bytes: directory.aggregate_bytes,
            directory_complete: directory.complete,
            last_event_id: directory.last_event_id,
            hash: 0,
        };
        fingerprint.hash = fingerprint.compute_hash();
        fingerprint
    }

    pub(super) fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(STORAGE_PATH_FINGERPRINT_LEN);
        bytes.push(STORAGE_PATH_FINGERPRINT_VERSION);
        bytes.push(self.kind_code);
        bytes.extend_from_slice(&self.mode.to_le_bytes());
        for value in [
            self.device,
            self.inode,
            self.logical_bytes,
            self.physical_bytes,
            self.modified_millis,
            self.changed_millis,
            self.birth_millis,
            self.flags,
            self.link_count,
            self.directory_child_count,
            self.directory_shallow_hash,
            self.directory_aggregate_bytes,
            if self.directory_complete { 1 } else { 0 },
            self.last_event_id,
            self.hash,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    pub(super) fn encoded_version(bytes: &[u8]) -> Option<u8> {
        bytes.first().copied()
    }

    #[cfg(test)]
    pub(super) fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != STORAGE_PATH_FINGERPRINT_LEN
            || bytes.first().copied()? != STORAGE_PATH_FINGERPRINT_VERSION
        {
            return None;
        }
        let kind_code = bytes[1];
        let mode = read_u32_le(bytes, 2)?;
        let mut offset = 6;
        let device = read_u64_le(bytes, offset)?;
        offset += 8;
        let inode = read_u64_le(bytes, offset)?;
        offset += 8;
        let logical_bytes = read_u64_le(bytes, offset)?;
        offset += 8;
        let physical_bytes = read_u64_le(bytes, offset)?;
        offset += 8;
        let modified_millis = read_u64_le(bytes, offset)?;
        offset += 8;
        let changed_millis = read_u64_le(bytes, offset)?;
        offset += 8;
        let birth_millis = read_u64_le(bytes, offset)?;
        offset += 8;
        let flags = read_u64_le(bytes, offset)?;
        offset += 8;
        let link_count = read_u64_le(bytes, offset)?;
        offset += 8;
        let directory_child_count = read_u64_le(bytes, offset)?;
        offset += 8;
        let directory_shallow_hash = read_u64_le(bytes, offset)?;
        offset += 8;
        let directory_aggregate_bytes = read_u64_le(bytes, offset)?;
        offset += 8;
        let directory_complete = read_u64_le(bytes, offset)? != 0;
        offset += 8;
        let last_event_id = read_u64_le(bytes, offset)?;
        offset += 8;
        let hash = read_u64_le(bytes, offset)?;
        let fingerprint = Self {
            kind_code,
            mode,
            device,
            inode,
            logical_bytes,
            physical_bytes,
            modified_millis,
            changed_millis,
            birth_millis,
            flags,
            link_count,
            directory_child_count,
            directory_shallow_hash,
            directory_aggregate_bytes,
            directory_complete,
            last_event_id,
            hash,
        };
        (fingerprint.compute_hash() == hash).then_some(fingerprint)
    }

    #[cfg(test)]
    pub(super) fn stable_hash(&self) -> u64 {
        self.hash
    }

    #[cfg(test)]
    pub(super) fn changed_since(&self, previous: &Self) -> bool {
        self.hash != previous.hash
    }

    fn compute_hash(&self) -> u64 {
        let mut hash = FNV_OFFSET_BASIS;
        mix_u8(&mut hash, STORAGE_PATH_FINGERPRINT_VERSION);
        mix_u8(&mut hash, self.kind_code);
        mix_u32(&mut hash, self.mode);
        for value in [
            self.device,
            self.inode,
            self.logical_bytes,
            self.physical_bytes,
            self.modified_millis,
            self.changed_millis,
            self.birth_millis,
            self.flags,
            self.link_count,
            self.directory_child_count,
            self.directory_shallow_hash,
            self.directory_aggregate_bytes,
            if self.directory_complete { 1 } else { 0 },
            self.last_event_id,
        ] {
            mix_u64(&mut hash, value);
        }
        hash
    }
}

pub(super) fn metadata_birth_millis(metadata: &fs::Metadata) -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        Some(metadata_time_millis(
            std::os::macos::fs::MetadataExt::st_birthtime(metadata),
            std::os::macos::fs::MetadataExt::st_birthtime_nsec(metadata),
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        metadata.created().ok().and_then(system_time_millis)
    }
}

fn metadata_flags(metadata: &fs::Metadata) -> u64 {
    #[cfg(target_os = "macos")]
    {
        std::os::macos::fs::MetadataExt::st_flags(metadata) as u64
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = metadata;
        0
    }
}

fn metadata_time_millis(seconds: i64, nanoseconds: i64) -> u64 {
    seconds
        .saturating_mul(1000)
        .saturating_add(nanoseconds.max(0) / 1_000_000)
        .max(0) as u64
}

#[cfg(not(target_os = "macos"))]
fn system_time_millis(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
}

fn shallow_directory_fingerprint(path: &Path) -> (u64, u64, bool) {
    let Ok(entries) = fs::read_dir(path) else {
        return (0, 0, false);
    };
    let mut child_count = 0u64;
    let mut selected = BinaryHeap::<(String, PathBuf)>::new();
    let mut complete = true;
    for entry in entries {
        let Ok(entry) = entry else {
            complete = false;
            continue;
        };
        child_count = child_count.saturating_add(1);
        if child_count > DIRECTORY_SHALLOW_FINGERPRINT_CHILD_LIMIT {
            complete = false;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if selected.len() < DIRECTORY_SHALLOW_FINGERPRINT_CHILD_LIMIT as usize {
            selected.push((name, entry.path()));
        } else if selected
            .peek()
            .is_some_and(|(largest_name, _)| name < *largest_name)
        {
            let _ = selected.pop();
            selected.push((name, entry.path()));
        }
    }
    let children = selected.into_sorted_vec();
    let mut hash = FNV_OFFSET_BASIS;
    for (name, path) in &children {
        let Ok(metadata) = fs::symlink_metadata(path) else {
            complete = false;
            continue;
        };
        mix_str(&mut hash, name);
        let child = StoragePathFingerprint::from_metadata(&metadata);
        mix_u64(&mut hash, child.compute_hash());
    }
    (child_count, hash, complete)
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn mix_u8(hash: &mut u64, value: u8) {
    *hash ^= value as u64;
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn mix_u32(hash: &mut u64, value: u32) {
    for byte in value.to_le_bytes() {
        mix_u8(hash, byte);
    }
}

fn mix_u64(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        mix_u8(hash, byte);
    }
}

fn mix_str(hash: &mut u64, value: &str) {
    for byte in value.as_bytes() {
        mix_u8(hash, *byte);
    }
}

#[cfg(test)]
fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(slice))
}

#[cfg(test)]
fn read_u64_le(bytes: &[u8], offset: usize) -> Option<u64> {
    let slice: [u8; 8] = bytes.get(offset..offset + 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(slice))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aetower-storage-fingerprint-{}-{name}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    #[test]
    fn storage_path_fingerprint_round_trips_binary_encoding() {
        let root = temp_test_dir("roundtrip");
        let file = root.join("artifact.bin");
        fs::write(&file, b"baseline").expect("write fixture");
        let metadata = fs::symlink_metadata(&file).expect("metadata");

        let fingerprint = StoragePathFingerprint::from_metadata(&metadata);
        let encoded = fingerprint.encode();
        let decoded = StoragePathFingerprint::decode(&encoded).expect("decode");

        assert_eq!(decoded, fingerprint);
        assert_eq!(decoded.stable_hash(), fingerprint.stable_hash());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_path_fingerprint_changes_when_file_content_changes() {
        let root = temp_test_dir("changes");
        let file = root.join("artifact.bin");
        fs::write(&file, b"baseline").expect("write baseline");
        let baseline = StoragePathFingerprint::from_metadata(
            &fs::symlink_metadata(&file).expect("baseline metadata"),
        );

        fs::write(&file, b"baseline plus more bytes").expect("write changed");
        let changed = StoragePathFingerprint::from_metadata(
            &fs::symlink_metadata(&file).expect("changed metadata"),
        );

        assert!(changed.changed_since(&baseline));
        assert_ne!(changed.stable_hash(), baseline.stable_hash());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_directory_fingerprint_tracks_shallow_children_and_context() {
        let root = temp_test_dir("directory");
        let directory = root.join("cache");
        fs::create_dir_all(&directory).expect("create cache directory");
        fs::write(directory.join("alpha.bin"), b"alpha").expect("write first child");
        let metadata = fs::symlink_metadata(&directory).expect("directory metadata");
        let baseline_directory =
            StorageDirectoryFingerprint::for_path(&directory, 16_384, true, 41);
        let baseline = StoragePathFingerprint::from_metadata_with_directory(
            &metadata,
            Some(baseline_directory),
        );
        let decoded = StoragePathFingerprint::decode(&baseline.encode()).expect("decode");

        assert_eq!(decoded.kind_code, 2);
        assert_eq!(decoded.directory_child_count, 1);
        assert_eq!(decoded.directory_aggregate_bytes, 16_384);
        assert!(decoded.directory_complete);
        assert_eq!(decoded.last_event_id, 41);
        assert_eq!(
            decoded.birth_millis,
            metadata_birth_millis(&metadata).unwrap_or_default()
        );
        assert_eq!(decoded.flags, metadata_flags(&metadata));

        fs::write(directory.join("beta.bin"), b"beta").expect("write second child");
        let changed_metadata = fs::symlink_metadata(&directory).expect("changed metadata");
        let changed_directory = StorageDirectoryFingerprint::for_path(&directory, 16_384, true, 41);
        let changed = StoragePathFingerprint::from_metadata_with_directory(
            &changed_metadata,
            Some(changed_directory),
        );
        assert!(changed.changed_since(&baseline));
        assert_eq!(changed.directory_child_count, 2);

        let event_changed = StoragePathFingerprint::from_metadata_with_directory(
            &changed_metadata,
            Some(StorageDirectoryFingerprint::for_path(
                &directory, 16_384, true, 42,
            )),
        );
        assert!(event_changed.changed_since(&changed));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_path_fingerprint_rejects_corrupt_encoding() {
        let root = temp_test_dir("corrupt");
        let file = root.join("artifact.bin");
        fs::write(&file, b"baseline").expect("write fixture");
        let metadata = fs::symlink_metadata(&file).expect("metadata");
        let mut encoded = StoragePathFingerprint::from_metadata(&metadata).encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;

        assert!(StoragePathFingerprint::decode(&encoded).is_none());
        let _ = fs::remove_dir_all(root);
    }
}
