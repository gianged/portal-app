//! Thin entry point; the composition root and serving loop live in [`server::run`].
//!
//! Runs on a thread with an 8 MiB stack: composing the full router produces deep
//! stack frames that overflow Windows' default 1 MiB main-thread stack in debug.
fn main() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("server-main".to_owned())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(server::run())
        })?
        .join()
        .expect("server thread panicked")
}
