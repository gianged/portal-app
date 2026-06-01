//! Thin entry point. The composition root and serving loop live in the `server`
//! library ([`server::run`]).
//!
//! Builds the runtime on a thread with a generous stack: composing the full
//! router (many nested route groups + middleware) produces deep stack frames in
//! debug builds that overflow Windows' default 1 MiB main-thread stack; an 8 MiB
//! stack gives the composition root ample headroom.
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
