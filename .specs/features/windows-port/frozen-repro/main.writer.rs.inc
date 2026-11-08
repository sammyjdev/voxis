
// ---- appended by scripts/windows/frozen-repro.sh to main.rs of the frozen SHA 889d48c ----
// Criterion 64 / review finding P5-6: criterion 18 through the frozen `writer`, the function
// `record` starts for each track. A track file that cannot be written ends the writer thread
// with a panic (`expect("create wav")`, `write_sample(..).unwrap()`); the plan wants an error.
// This is a create failure on an unwritable path, not a full disk: the full-disk case needs an
// injected writer and is proven only at the candidate's seam.
#[cfg(test)]
mod frozen_repro {
    use super::*;

    fn quiet_chunks() -> mpsc::Receiver<Chunk> {
        let (tx, rx) = mpsc::channel();
        tx.send(Chunk { at: 0.0, mono: vec![0.25; 160] }).unwrap();
        rx
    }

    #[test]
    fn control_a_writable_track_is_written_and_finalized() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mic.wav");

        let (written, _peak) = writer(path.clone(), 16_000, quiet_chunks()).join().expect("a writable track");

        assert_eq!(written, 160);
        assert_eq!(hound::WavReader::open(&path).unwrap().len(), 160);
    }

    #[test]
    fn a_track_that_cannot_be_created_ends_in_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("no-such-folder").join("mic.wav");

        let ended = writer(path, 16_000, quiet_chunks()).join();

        assert!(ended.is_ok(), "the writer thread panicked");
    }
}
