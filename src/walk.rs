use std::{path::PathBuf, sync::{mpsc::{channel, Receiver, RecvTimeoutError, Sender}, Arc, atomic::{AtomicBool, Ordering}}, time::{Instant, Duration}, io::{Write, self}, mem, thread};

use ignore::{overrides::OverrideBuilder, WalkBuilder};
use anyhow::{anyhow, Result};


use crate::{exit_codes::ExitCode, dir_entry::DirEntry, error::print_error, output};

/// Default duration until output buffering switches to streaming.
pub const DEFAULT_MAX_BUFFER_TIME: Duration = Duration::from_millis(100);
/// Maximum size of the output buffer before flushing results to the console
pub const MAX_BUFFER_LENGTH: usize = 1000;

pub fn scan(path_vec: &[PathBuf]) -> Result<ExitCode> {
    let mut path_iter = path_vec.iter();
    let first_path_buf = path_iter
        .next()
        .expect("Error: Path vector can not be empty");
    let (tx, rx) = channel();

    let mut override_builder = OverrideBuilder::new(first_path_buf.as_path());
    let overrides = override_builder
        .build()
        .map_err(|_| anyhow!("Mismatch in exclude patterns"))?;
    let mut walker = WalkBuilder::new(first_path_buf.as_path());
    walker
        .hidden(true)
        .ignore(false)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .overrides(overrides)
        .follow_links(true);

    let parallel_walker = walker.threads(4).build_parallel();
    // Flag for cleanly shutting down the parallel walk
    let quit_flag = Arc::new(AtomicBool::new(false));
    // Flag specifically for quitting due to ^C
    let interrupt_flag = Arc::new(AtomicBool::new(false));

    // Spawn the thread that receives all results through the channel.
    let receiver_thread = spawn_receiver(&quit_flag, &interrupt_flag, rx);

    // Spawn the sender threads.
    spawn_senders(&quit_flag, parallel_walker, tx);

    // Wait for the receiver thread to print out all results.
    let exit_code = receiver_thread.join().unwrap();
    if interrupt_flag.load(Ordering::Relaxed) {
        Ok(ExitCode::KilledBySigint)
    } else {
        Ok(exit_code)
    }
}

#[derive(PartialEq)]
enum ReceiverMode {
    /// Receiver is still buffering in order to sort the results, if the search finishes fast
    /// enough.
    Buffering,

    /// Receiver is directly printing results to the output.
    Streaming,
}

pub enum WorkerResult {
    Entry(DirEntry),
    Error(ignore::Error),
}

struct ReceiverBuffer<W> {
    /// The configuration.
    // config: Arc<Config>,
    /// For shutting down the senders.
    quit_flag: Arc<AtomicBool>,
    /// The ^C notifier.
    interrupt_flag: Arc<AtomicBool>,
    /// Receiver for worker results.
    rx: Receiver<WorkerResult>,
    /// Standard output.
    stdout: W,
    /// The current buffer mode.
    mode: ReceiverMode,
    /// The deadline to switch to streaming mode.
    deadline: Instant,
    /// The buffer of quickly received paths.
    buffer:  Vec<DirEntry>,
    /// Result count.
    num_results: usize,
}

impl<W: Write> ReceiverBuffer<W> {
    fn new(
        // config: Arc<Config>,
        quit_flag: Arc<AtomicBool>,
        interrupt_flag: Arc<AtomicBool>,
        rx: Receiver<WorkerResult>,
        stdout: W,
    ) -> Self {
        let max_buffer_time = DEFAULT_MAX_BUFFER_TIME;
        let deadline = Instant::now() + max_buffer_time;

        Self {
            // config,
            quit_flag,
            interrupt_flag,
            rx,
            stdout,
            mode: ReceiverMode::Buffering,
            deadline,
            buffer: Vec::with_capacity(MAX_BUFFER_LENGTH),
            num_results: 0,
        }
    }

    fn process(&mut self) -> ExitCode {
        loop {
            if let Err(ec) = self.poll() {
                self.quit_flag.store(true, Ordering::Relaxed);
                return ec
            }
        }
    }

    fn poll(&mut self) -> Result<(), ExitCode> {
        match self.recv() {
            Ok(WorkerResult::Entry(dir_entry)) => {
                // if self.config.quiet {
                //     return Err(ExitCode::HasResults(true));
                // }

                match self.mode {
                    ReceiverMode::Buffering => {
                        self.buffer.push(dir_entry);
                        if self.buffer.len() > MAX_BUFFER_LENGTH {
                            self.stream()?;
                        }
                    }
                    ReceiverMode::Streaming => {
                        self.print(&dir_entry)?;
                        self.flush()?;
                    }
                }

                self.num_results += 1;
                // if let Some(max_results) = self.config.max_results {
                //     if self.num_results >= max_results {
                //         return self.stop();
                //     }
                // }
            }
            Ok(WorkerResult::Error(err)) => {
                print_error(err.to_string());
                // if self.config.show_filesystem_errors {
                //     print_error(err.to_string());
                // }
            }
            Err(RecvTimeoutError::Timeout) => {
                self.stream()?;
            }
            Err(RecvTimeoutError::Disconnected) => {
                return self.stop();
            }
        }
        Ok(())
    }

    fn recv(&self) -> Result<WorkerResult, RecvTimeoutError> {
        match self.mode {
            ReceiverMode::Buffering => {
                // Wait at most until we should switch to streaming
                let now = Instant::now();
                self.deadline
                    .checked_duration_since(now)
                    .ok_or(RecvTimeoutError::Timeout)
                    .and_then(|t| self.rx.recv_timeout(t))
            }
            ReceiverMode::Streaming => {
                // Wait however long it takes for a result
                Ok(self.rx.recv()?)
            }
        }
    }

    fn stream(&mut self) -> Result<(), ExitCode> {
        self.mode = ReceiverMode::Streaming;

        let buffer = mem::take(&mut self.buffer);
        for path in buffer {
            self.print(&path)?;
        }

        self.flush()
    }

    fn print(&mut self, entry: &DirEntry) -> Result<(), ExitCode>{
        output::print_entry(&mut self.stdout, entry);
        Ok(())
    }

    /// Stop looping.
    fn stop(&mut self) -> Result<(), ExitCode> {
        if self.mode == ReceiverMode::Buffering {
            self.buffer.sort();
            self.stream()?;
        }
        Err(ExitCode::HasResults(self.num_results > 0))
        // if self.config.quiet {
        //     Err(ExitCode::HasResults(self.num_results > 0))
        // } else {
        //     Err(ExitCode::Success)
        // }
    }

    /// Flush stdout if necessary.
    fn flush(&mut self) -> Result<(), ExitCode> {
        if self.stdout.flush().is_err() {
            return Err(ExitCode::GeneralError);
        }
        // if self.config.interactive_terminal && self.stdout.flush().is_err() {
        //     // Probably a broken pipe. Exit gracefully.
        //     return Err(ExitCode::GeneralError);
        // }
        Ok(())
    }

}

fn spawn_receiver(
    // config: &Arc<Config>,
    quit_flag: &Arc<AtomicBool>,
    interrupt_flag: &Arc<AtomicBool>,
    rx: Receiver<WorkerResult>,
) -> thread::JoinHandle<ExitCode> {
    // let configs = Arc::clone(config);
    let quit_flag = Arc::clone(quit_flag);
    let interrupt_flag = Arc::clone(interrupt_flag);

    let threads = 4;
    thread::spawn(move || {
        let stdout = io::stdout();
        let stdout = stdout.lock();
        let stdout = io::BufWriter::new(stdout);
        let mut rxbuffer = ReceiverBuffer::new(quit_flag, interrupt_flag, rx, stdout);
            rxbuffer.process()
    })
}

fn spawn_senders(
    // config: &Arc<Config>,
    quit_flag: &Arc<AtomicBool>,
    // pattern: Arc<Regex>,
    parallel_walker: ignore::WalkParallel,
    tx: Sender<WorkerResult>,
)  {
    parallel_walker.run(|| {
        // let config = Arc::clone(config);
        // let pattern = Arc::clone(&pattern);
        let tx_thread = tx.clone();
        let quit_flag = Arc::clone(quit_flag);
        Box::new(move | entry_o| {
            let entry = match entry_o {
                Ok(ref e) if e.depth() == 0 => {
                    // Skip the root directory entry.
                    return ignore::WalkState::Continue;
                }
                Ok(e) => DirEntry::normal(e),
                Err(err) => {
                    return match tx_thread.send(WorkerResult::Error(err)) {
                        Ok(_) => ignore::WalkState::Continue,
                        Err(_) => ignore::WalkState::Quit,
                    }
                }
            };
            let entry_path = entry.path();
            let send_result = tx_thread.send(WorkerResult::Entry(entry));
            if send_result.is_err() {
                return ignore::WalkState::Quit;
            }
            ignore::WalkState::Continue
        })
    })
}
