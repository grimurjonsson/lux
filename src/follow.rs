use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};

use crate::engine::Engine;
use crate::filter::LineFilter;
use crate::slow::SlowLineAnnotator;
use crate::trigger::{OutputDecision, TriggerFilter};

/// How to follow a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowMode {
    /// Follow by file descriptor (-f). Keeps reading the same fd.
    /// Exits when file is deleted and EOF is reached.
    Descriptor,
    /// Follow by file name (-F). Detects rotation via inode identity.
    /// Handles rename/create and copytruncate rotation strategies.
    Name,
}

/// Identity of a file based on device and inode numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileIdentity {
    dev: u64,
    ino: u64,
}

impl FileIdentity {
    /// Extract file identity from metadata.
    fn from_metadata(meta: &std::fs::Metadata) -> Self {
        Self {
            dev: meta.dev(),
            ino: meta.ino(),
        }
    }

    /// Check if this identity matches another.
    fn matches(&self, other: &FileIdentity) -> bool {
        self.dev == other.dev && self.ino == other.ino
    }
}

/// Read all new lines from the reader and write colorized output.
///
/// Uses `read_line()` in a loop (NOT `lines()` iterator which terminates at EOF).
/// Returns the number of lines read.
fn read_new_lines(
    reader: &mut BufReader<File>,
    engine: &mut Engine,
    writer: &mut BufWriter<impl Write>,
    mut trigger: Option<&mut TriggerFilter>,
    filter: Option<&LineFilter>,
    mut slow: Option<&mut SlowLineAnnotator>,
) -> anyhow::Result<usize> {
    let mut line = String::new();
    let mut count = 0;
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break; // EOF -- no more data right now
        }
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        // Apply line filter before coloring and trigger processing
        if let Some(f) = filter {
            if f.is_active() && !f.should_show(trimmed) {
                count += 1;
                continue;
            }
        }
        let result = engine.apply(trimmed);
        if let Some(ref mut tf) = trigger {
            match tf.process_line(trimmed, result.flatten()) {
                OutputDecision::Pass(out_lines) => {
                    for l in out_lines {
                        if let Some(ref mut ann) = slow {
                            if let Some(prev) = ann.annotate(&l) {
                                writeln!(writer, "{prev}")?;
                            }
                        } else {
                            writeln!(writer, "{l}")?;
                        }
                    }
                }
                OutputDecision::Flush(lines) => {
                    for l in lines {
                        if let Some(ref mut ann) = slow {
                            if let Some(prev) = ann.annotate(&l) {
                                writeln!(writer, "{prev}")?;
                            }
                        } else {
                            writeln!(writer, "{l}")?;
                        }
                    }
                }
                OutputDecision::Suppress => {}
            }
        } else {
            for l in &result.before {
                writeln!(writer, "{l}")?;
            }
            if let Some(ref mut ann) = slow {
                if let Some(prev) = ann.annotate(&result.line) {
                    writeln!(writer, "{prev}")?;
                }
            } else {
                writeln!(writer, "{}", result.line)?;
            }
            for l in &result.after {
                writeln!(writer, "{l}")?;
            }
        }
        count += 1;
    }
    // Flush the last buffered line from the slow annotator at end of batch
    if let Some(ref mut ann) = slow {
        if let Some(last) = ann.flush() {
            writeln!(writer, "{last}")?;
        }
    }
    if count > 0 {
        writer.flush()?;
    }
    Ok(count)
}

/// Drain all pending events from the channel (debouncing).
fn drain_events(rx: &mpsc::Receiver<notify::Result<notify::Event>>) {
    while rx.try_recv().is_ok() {}
}

/// Follow a file that is already open.
///
/// The file should already be positioned correctly (typically at EOF after reading initial lines).
pub fn run(
    path: &Path,
    mode: FollowMode,
    file: File,
    engine: &mut Engine,
    writer: &mut BufWriter<impl Write>,
    mut trigger: Option<TriggerFilter>,
    filter: Option<&LineFilter>,
    mut slow: Option<SlowLineAnnotator>,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(file);

    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;

    match mode {
        FollowMode::Descriptor => {
            // Watch the file directly for descriptor mode
            if path.exists() {
                watcher.watch(path, RecursiveMode::NonRecursive)?;
            }
        }
        FollowMode::Name => {
            // Watch the parent directory for name mode (to detect file recreation)
            let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }
    }

    // Track file position for copytruncate detection
    let mut last_pos = reader.stream_position()?;
    let mut identity = FileIdentity::from_metadata(&reader.get_ref().metadata()?);
    let mut consecutive_timeouts = 0u32;

    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Ok(event)) => {
                consecutive_timeouts = 0;

                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Access(_) => {
                        // Debounce: drain pending events before reading
                        drain_events(&rx);

                        if mode == FollowMode::Name {
                            // Check for rotation (inode change)
                            if let Ok(path_meta) = std::fs::metadata(path) {
                                let path_identity = FileIdentity::from_metadata(&path_meta);
                                if !identity.matches(&path_identity) {
                                    // Rotation detected -- drain old fd first
                                    read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;

                                    // Open new file from beginning
                                    let new_file = File::open(path)?;
                                    identity = FileIdentity::from_metadata(&new_file.metadata()?);
                                    reader = BufReader::new(new_file);
                                    read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                                    last_pos = reader.stream_position()?;
                                    continue;
                                }
                            }

                            // Check for copytruncate
                            if let Ok(meta) = reader.get_ref().metadata() {
                                let current_size = meta.len();
                                if current_size < last_pos {
                                    // File was truncated -- seek to beginning
                                    reader.seek(SeekFrom::Start(0))?;
                                    // Clear BufReader internal buffer after seek
                                    reader = BufReader::new(reader.into_inner());
                                }
                            }
                        } else {
                            // Descriptor mode: check for copytruncate too
                            if let Ok(meta) = reader.get_ref().metadata() {
                                let current_size = meta.len();
                                if current_size < last_pos {
                                    reader.seek(SeekFrom::Start(0))?;
                                    reader = BufReader::new(reader.into_inner());
                                }
                            }
                        }

                        read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                        last_pos = reader.stream_position()?;
                    }
                    EventKind::Remove(_) if mode == FollowMode::Name => {
                        // File disappeared in -F mode
                        if !path.exists() {
                            eprintln!(
                                "lux: '{}' has disappeared; waiting...",
                                path.display()
                            );
                            // Poll until file reappears
                            loop {
                                std::thread::sleep(Duration::from_secs(1));
                                if path.exists() {
                                    eprintln!(
                                        "lux: '{}' has appeared; following new file",
                                        path.display()
                                    );
                                    let new_file = File::open(path)?;
                                    identity =
                                        FileIdentity::from_metadata(&new_file.metadata()?);
                                    reader = BufReader::new(new_file);
                                    read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                                    last_pos = reader.stream_position()?;

                                    // Re-watch parent directory
                                    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
                                    let _ = watcher.unwatch(parent);
                                    watcher.watch(parent, RecursiveMode::NonRecursive)?;
                                    break;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Err(e)) => {
                eprintln!("lux: watch error: {e}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                consecutive_timeouts += 1;

                match mode {
                    FollowMode::Descriptor => {
                        // Try reading any pending data
                        let count = read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                        if count > 0 {
                            last_pos = reader.stream_position()?;
                            consecutive_timeouts = 0;
                        }

                        // After many timeouts with no data, check if file was deleted
                        // and no more data to read -- exit cleanly
                        if consecutive_timeouts > 20 && !path.exists() {
                            // File deleted, no more events, try one last read
                            let final_count = read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                            if final_count == 0 {
                                return Ok(());
                            }
                            last_pos = reader.stream_position()?;
                        }
                    }
                    FollowMode::Name => {
                        // Periodic checks for rotation and copytruncate
                        if let Ok(path_meta) = std::fs::metadata(path) {
                            let path_identity = FileIdentity::from_metadata(&path_meta);
                            if !identity.matches(&path_identity) {
                                // Rotation detected
                                read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                                let new_file = File::open(path)?;
                                identity =
                                    FileIdentity::from_metadata(&new_file.metadata()?);
                                reader = BufReader::new(new_file);
                                read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                                last_pos = reader.stream_position()?;
                                continue;
                            }

                            // Check copytruncate
                            let current_size = path_meta.len();
                            if current_size < last_pos {
                                reader.seek(SeekFrom::Start(0))?;
                                reader = BufReader::new(reader.into_inner());
                            }
                        }

                        // Try reading
                        let count = read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                        if count > 0 {
                            last_pos = reader.stream_position()?;
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Watcher disconnected -- do final read and exit
                read_new_lines(&mut reader, engine, writer, trigger.as_mut(), filter, slow.as_mut())?;
                return Ok(());
            }
        }
    }
}

/// Follow a file that does not yet exist (wait for creation).
///
/// Used for -F mode or bare file when the file is missing at startup.
/// Polls every 1 second until the file appears, then transitions to normal Name-mode following.
pub fn run_waiting(
    path: &Path,
    engine: &mut Engine,
    writer: &mut BufWriter<impl Write>,
    trigger: Option<TriggerFilter>,
    filter: Option<&LineFilter>,
    slow: Option<SlowLineAnnotator>,
) -> anyhow::Result<()> {
    // Poll until file appears
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if path.exists() {
            eprintln!(
                "lux: '{}' has appeared; following new file",
                path.display()
            );
            let file = File::open(path)?;
            return run(path, FollowMode::Name, file, engine, writer, trigger, filter, slow);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_identity_from_metadata() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let meta = std::fs::metadata(tmp.path()).unwrap();
        let id = FileIdentity::from_metadata(&meta);
        assert!(id.dev > 0 || id.ino > 0, "should have non-zero identity");
    }

    #[test]
    fn file_identity_matches_same_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let meta = std::fs::metadata(tmp.path()).unwrap();
        let id1 = FileIdentity::from_metadata(&meta);
        let id2 = FileIdentity::from_metadata(&meta);
        assert!(id1.matches(&id2), "same metadata should match");
    }

    #[test]
    fn file_identity_differs_for_different_files() {
        let tmp1 = tempfile::NamedTempFile::new().unwrap();
        let tmp2 = tempfile::NamedTempFile::new().unwrap();
        let meta1 = std::fs::metadata(tmp1.path()).unwrap();
        let meta2 = std::fs::metadata(tmp2.path()).unwrap();
        let id1 = FileIdentity::from_metadata(&meta1);
        let id2 = FileIdentity::from_metadata(&meta2);
        assert!(
            !id1.matches(&id2),
            "different files should have different identity"
        );
    }

    #[test]
    fn file_identity_equality() {
        let id1 = FileIdentity { dev: 1, ino: 100 };
        let id2 = FileIdentity { dev: 1, ino: 100 };
        let id3 = FileIdentity { dev: 1, ino: 200 };
        let id4 = FileIdentity { dev: 2, ino: 100 };
        assert!(id1.matches(&id2));
        assert!(!id1.matches(&id3));
        assert!(!id1.matches(&id4));
    }

    #[test]
    fn follow_mode_enum() {
        let d = FollowMode::Descriptor;
        let n = FollowMode::Name;
        assert_ne!(d, n);
        assert_eq!(d, FollowMode::Descriptor);
        assert_eq!(n, FollowMode::Name);
    }
}
