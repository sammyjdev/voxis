
// ---- appended by scripts/windows/frozen-repro.sh to main.rs of the frozen SHA 889d48c (certificates round) ----
// Criterion 18 through the frozen `writer`, with a failure DURING writing and no adapter: the frozen
// function runs unmodified on a real `File`; the operating system makes the write fail by itself.
//
// How: the test creates the track file, then locks the byte range from 1 MiB onwards through a second
// handle (`LockFileEx`, declared here, test only). The writer's `File::create` and the first 1 MiB of
// samples succeed; the first 8 KiB flush that reaches the locked range fails with a lock violation.
// Nothing fills a disk. The frozen `write_sample(..).unwrap()` turns that failure into a panic.
#[cfg(all(test, windows))]
mod frozen_repro_midwrite {
    use super::*;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut std::ffi::c_void,
    }

    extern "system" {
        fn LockFileEx(file: *mut std::ffi::c_void, flags: u32, reserved: u32, low: u32, high: u32, overlapped: *mut Overlapped) -> i32;
    }

    const LOCK_FROM: u32 = 1 << 20;

    /// The track file, created empty and locked from `LOCK_FROM` on by the returned handle.
    fn locked_track(dir: &std::path::Path) -> (std::path::PathBuf, std::fs::File) {
        let path = dir.join("mic.wav");
        let locker = std::fs::File::create(&path).unwrap();
        let mut o = Overlapped { internal: 0, internal_high: 0, offset: LOCK_FROM, offset_high: 0, event: std::ptr::null_mut() };
        // LOCKFILE_FAIL_IMMEDIATELY | LOCKFILE_EXCLUSIVE_LOCK, 4 GiB
        let ok = unsafe { LockFileEx(locker.as_raw_handle() as *mut _, 3, 0, u32::MAX, 0, &mut o) };
        assert_ne!(ok, 0, "LockFileEx failed: {}", std::io::Error::last_os_error());
        (path, locker)
    }

    fn chunks(count: usize) -> mpsc::Receiver<Chunk> {
        let (tx, rx) = mpsc::channel();
        for i in 0..count {
            tx.send(Chunk { at: i as f64 * 0.25, mono: vec![0.25; 4096] }).unwrap();
        }
        rx
    }

    #[test]
    fn control_a_track_that_stays_below_the_locked_range_is_written_and_finalized() {
        let tmp = tempfile::tempdir().unwrap();
        let (path, _locker) = locked_track(tmp.path());

        let (written, _peak) = writer(path.clone(), 16_000, chunks(10)).join().expect("a track below the lock is written");

        assert_eq!(written, 10 * 4096);
        assert_eq!(hound::WavReader::open(&path).unwrap().len(), 10 * 4096);
    }

    #[test]
    fn a_write_that_fails_in_the_middle_of_a_track_ends_in_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let (path, _locker) = locked_track(tmp.path());

        // 100 chunks of 4096 f32 samples: about 1.6 MiB, past the locked range
        let ended = writer(path.clone(), 16_000, chunks(100)).join();

        let panic_text = ended.as_ref().err().map(|p| {
            p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
        });
        assert!(
            ended.is_ok(),
            "the writer thread panicked after {} bytes were on disk: {:?}",
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            panic_text
        );
    }
}
