//! Stable content fingerprints for local source binding, not cryptographic integrity.
//!
//! FNV-1a is suitable for detecting ordinary source changes. It is not a signature,
//! an authentication mechanism or protection against deliberately crafted collisions.
use std::{fs::File, io::{ErrorKind, Read}, path::Path};

const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

fn update(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
    }
    hash
}

fn format_fingerprint(hash: u64) -> String {
    format!("fnv1a64:{hash:016x}")
}

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    format_fingerprint(update(OFFSET_BASIS, bytes))
}

/// Hash the file contents in bounded memory without depending on path or metadata.
pub fn fingerprint_file(path: &Path) -> Result<String, String> {
    let mut source = File::open(path)
        .map_err(|error| format!("Lecture de la source impossible ({}): {error}", path.display()))?;
    let mut hash = OFFSET_BASIS;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => hash = update(hash, &buffer[..length]),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("Lecture de la source interrompue ({}): {error}", path.display())),
        }
    }
    Ok(format_fingerprint(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::OpenOptions, io::Write, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    struct TemporarySource(PathBuf);

    impl TemporarySource {
        fn create(contents: &[u8]) -> Self {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("dper-source-test-{}-{stamp}-{}.bin",
                std::process::id(), NEXT_FILE.fetch_add(1, Ordering::Relaxed)));
            let mut file = OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
            file.write_all(contents).unwrap();
            Self(path)
        }
    }

    impl Drop for TemporarySource {
        fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
    }

    #[test]
    fn fingerprints_match_published_fnv1a_vectors_and_have_stable_format() {
        assert_eq!(fingerprint_bytes(b""), "fnv1a64:cbf29ce484222325");
        assert_eq!(fingerprint_bytes(b"hello"), "fnv1a64:a430d84680aabd0b");
    }

    #[test]
    fn streaming_matches_bytes_across_multiple_buffers() {
        let data = (0..200_003).map(|index| (index % 251) as u8).collect::<Vec<_>>();
        let source = TemporarySource::create(&data);
        assert_eq!(fingerprint_file(&source.0).unwrap(), fingerprint_bytes(&data));
        let empty = TemporarySource::create(b"");
        assert_eq!(fingerprint_file(&empty.0).unwrap(), fingerprint_bytes(b""));
    }

    #[test]
    fn equal_length_source_changes_change_the_fingerprint() {
        let source = TemporarySource::create(b"first version");
        let before = fingerprint_file(&source.0).unwrap();
        std::fs::write(&source.0, b"other version").unwrap();
        let after = fingerprint_file(&source.0).unwrap();
        assert_ne!(before, after);
        assert_eq!(after, fingerprint_bytes(b"other version"));
    }

    #[test]
    fn missing_source_returns_an_error() {
        let source = TemporarySource::create(b"temporary");
        std::fs::remove_file(&source.0).unwrap();
        assert!(fingerprint_file(&source.0).is_err());
    }
}

